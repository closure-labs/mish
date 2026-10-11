{ pkgs, self }:
pkgs.testers.nixosTest {
  name = "mish-event-appliance";
  nodes.machine = { pkgs, ... }: {
    imports = [ self.nixosModules.vestaforge ];
    nixpkgs.overlays = [ self.overlays.default ];
    environment.systemPackages = [
      pkgs.curl
      pkgs.jq
    ];
    services.vestaforge = {
      enable = true;
      localAccess.enable = true;
      radio.simulator.enable = true;
      radio.providers = [ "meshtastic-simulated" ];
    };
    system.stateVersion = "26.05";
  };
  testScript = ''
    machine.start()
    machine.wait_for_unit("vestaforge-control.service")
    machine.wait_for_open_port(8787)
    machine.wait_for_unit("caddy.service")
    machine.wait_for_open_port(443)
    machine.succeed("curl --fail --silent --cacert /var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt --resolve mish.home.arpa:443:127.0.0.1 https://mish.home.arpa/health")
    machine.succeed("curl --fail --silent http://127.0.0.1:8787/health")
    machine.succeed("test $(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8787/api/messages) = 401")
    machine.succeed("curl --fail --silent http://127.0.0.1:8787/public/announcements")
    import json
    operator = json.loads(machine.succeed("vfctl --state-dir /var/lib/vestaforge commission --name Operator"))
    token = operator["token"]
    def request(method, path, payload=None, bearer=token):
        command = "curl --fail --silent -X " + method + " -H 'Authorization: Bearer " + bearer + "' -H 'Content-Type: application/json'"
        if payload is not None:
            import shlex
            command += " --data " + shlex.quote(json.dumps(payload))
        return json.loads(machine.succeed(command + " http://127.0.0.1:8787" + path))
    staff = request("POST", "/api/staff", {"name": "Staff", "role": "staff"})
    request("POST", "/api/messages", {"text": "Private restart test"})
    request("POST", "/api/messages", {"text": "Staff reply"}, staff["token"])
    expires = int(machine.succeed("date +%s").strip()) + 3600
    alert = request("POST", "/api/alerts", {"text": "Find supervisor", "responder": staff["id"], "kind": "alert", "expires": expires})
    request("POST", "/api/alerts/" + alert["id"] + "/ack", {}, staff["token"])
    machine.succeed("systemctl restart vestaforge-control.service")
    machine.wait_for_unit("vestaforge-control.service")
    machine.wait_for_open_port(8787)
    messages = request("GET", "/api/messages")
    assert "Private restart test" in json.dumps(messages)
    assert "Staff reply" in json.dumps(messages)
    restored_alert = next(item for item in messages["messages"] if item["id"] == alert["id"])
    assert any(ack["actor"] == staff["id"] for ack in restored_alert["acknowledgements"])
    assert "Private restart test" not in machine.succeed("curl --fail --silent http://127.0.0.1:8787/public/announcements")
  '';
}
