{ pkgs, self }:
pkgs.testers.nixosTest {
  name = "mish-event-appliance";
  nodes.machine = {
    imports = [ self.nixosModules.mish-simulation ];
  };
  testScript = ''
    machine.start()
    machine.wait_for_unit("mish-control.service")
    machine.wait_for_open_port(8787)
    machine.wait_for_unit("caddy.service")
    machine.wait_for_open_port(443)
    machine.succeed("curl --fail --silent --cacert /var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt --resolve mish.home.arpa:443:127.0.0.1 https://mish.home.arpa/health")
    machine.succeed("curl --fail --silent http://127.0.0.1:8787/health")
    machine.succeed("test $(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8787/api/messages) = 401")
    machine.succeed("curl --fail --silent http://127.0.0.1:8787/public/announcements")
    import json
    assert json.loads(machine.succeed("curl --fail --silent http://127.0.0.1:8787/public/channels")) == {"channels": []}
    assert "Event rooms" in machine.succeed("curl --fail --silent http://127.0.0.1:8787/public")
    assert "chooseChannel" in machine.succeed("curl --fail --silent http://127.0.0.1:8787/channel-selection.js")
    machine.succeed("test $(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8787/api/channels) = 401")
    operator = json.loads(machine.succeed("mish --state-dir /var/lib/mish commission --name Operator"))
    token = operator["token"]
    def request(method, path, payload=None, bearer=token):
        command = "curl --fail --silent -X " + method + " -H 'Authorization: Bearer " + bearer + "' -H 'Content-Type: application/json'"
        if payload is not None:
            import shlex
            command += " --data " + shlex.quote(json.dumps(payload))
        return json.loads(machine.succeed(command + " http://127.0.0.1:8787" + path))
    def guest_request(method, path, payload=None):
        import shlex
        command = "curl --fail --silent --cacert /var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt --resolve mish.home.arpa:443:127.0.0.1 -c /tmp/guest.cookies -b /tmp/guest.cookies -H 'Origin: https://mish.home.arpa' -H 'Content-Type: application/json' -X " + method
        if payload is not None:
            command += " --data " + shlex.quote(json.dumps(payload))
        return json.loads(machine.succeed(command + " https://mish.home.arpa" + path))
    request("POST", "/api/community/manage", {"action":"configure", "enabled":True, "enroll_per_hour":120, "posts_per_minute":30, "queue_limit":10})
    room = request("POST", "/api/community/manage", {"action":"create", "name":"Event lobby", "public":True, "channel":None})["id"]
    guest_request("POST", "/api/community/session", {"nickname":"Attendee"})
    guest_request("POST", "/api/community/rooms/" + room + "/membership")
    accepted = guest_request("POST", "/api/community/rooms/" + room + "/messages", {"key":"vm-retry", "text":"Local restart test"})
    assert accepted["delivery"] == "local"
    staff = request("POST", "/api/staff", {"name": "Staff", "role": "staff"})
    request("POST", "/api/messages", {"text": "Private restart test"})
    request("POST", "/api/messages", {"text": "Staff reply"}, staff["token"])
    expires = int(machine.succeed("date +%s").strip()) + 3600
    alert = request("POST", "/api/alerts", {"text": "Find supervisor", "responder": staff["id"], "kind": "alert", "expires": expires})
    request("POST", "/api/alerts/" + alert["id"] + "/ack", {}, staff["token"])
    machine.succeed("systemctl restart mish-control.service")
    machine.wait_for_unit("mish-control.service")
    machine.wait_for_open_port(8787)
    assert guest_request("POST", "/api/community/rooms/" + room + "/messages", {"key":"vm-retry", "text":"Local restart test"}) == accepted
    assert "Local restart test" in json.dumps(guest_request("GET", "/api/community/rooms/" + room + "/messages"))
    messages = request("GET", "/api/messages")
    assert "Local restart test" not in json.dumps(messages)
    assert "Private restart test" in json.dumps(messages)
    assert "Staff reply" in json.dumps(messages)
    restored_alert = next(item for item in messages["messages"] if item["id"] == alert["id"])
    assert any(ack["actor"] == staff["id"] for ack in restored_alert["acknowledgements"])
    assert "Private restart test" not in machine.succeed("curl --fail --silent http://127.0.0.1:8787/public/announcements")
    assert json.loads(machine.succeed("curl --fail --silent http://127.0.0.1:8787/public/channels")) == {"channels": []}

    announcement = request("POST", "/api/announcements", {"text": "Water station open"})
    machine.succeed("systemctl stop mish-control.service")
    snapshot_queued = json.loads(machine.succeed("runuser -u mish -- mish --state-dir /var/lib/mish message --author 'Operator' --text 'Queued at snapshot time'"))
    machine.succeed("runuser -u mish -- mish --state-dir /var/lib/mish backup --output /tmp/event-backup.sqlite")
    machine.succeed("systemctl start mish-control.service")
    machine.wait_for_open_port(8787)
    machine.wait_until_succeeds("curl --fail --silent -H 'Authorization: Bearer " + token + "' http://127.0.0.1:8787/api/messages | jq -e '.messages[] | select(.id == \"" + snapshot_queued["id"] + "\") | .state == \"submitted\"'")
    machine.succeed("systemctl stop mish-control.service")
    machine.succeed("mv /var/lib/mish /var/lib/mish-before-restore")
    machine.succeed("install -d -m 0700 -o mish -g mish /var/lib/mish")
    recovery = json.loads(machine.succeed("runuser -u mish -- mish --state-dir /var/lib/mish restore --input /tmp/event-backup.sqlite"))
    assert recovery["quarantined"] >= 1
    machine.fail("runuser -u mish -- mish --state-dir /var/lib/mish restore --input /tmp/event-backup.sqlite")
    machine.succeed("systemctl start mish-control.service")
    machine.wait_for_open_port(8787)
    assert guest_request("POST", "/api/community/rooms/" + room + "/messages", {"key":"vm-retry", "text":"Local restart test"}) == accepted
    recovered = request("GET", "/api/messages", bearer=staff["token"])["messages"]
    snapshot_record = next(item for item in recovered if item["id"] == snapshot_queued["id"])
    assert snapshot_record["state"] == "uncertain"
    assert snapshot_record["delivery"]["uncertainAt"] is not None
    recovered_alert = next(item for item in recovered if item["id"] == alert["id"])
    assert any(ack["actor"] == staff["id"] for ack in recovered_alert["acknowledgements"])
    public = json.loads(machine.succeed("curl --fail --silent http://127.0.0.1:8787/public/announcements"))
    assert public["announcements"] == [announcement]
    assert "Queued at snapshot time" not in json.dumps(public)
    fresh = request("POST", "/api/messages", {"text": "New message after recovery"}, staff["token"])
    machine.wait_until_succeeds("curl --fail --silent -H 'Authorization: Bearer " + token + "' http://127.0.0.1:8787/api/messages | jq -e '.messages[] | select(.id == \"" + fresh["id"] + "\") | .state == \"submitted\"'")
    final_messages = request("GET", "/api/messages")["messages"]
    assert next(item for item in final_messages if item["id"] == snapshot_queued["id"])["state"] == "uncertain"
    machine.succeed("test -f /var/lib/mish-before-restore/events.sqlite")
    machine.succeed("curl --fail --silent --cacert /var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt --resolve mish.home.arpa:443:127.0.0.1 https://mish.home.arpa/health")
  '';
}
