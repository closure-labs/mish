{ pkgs, self }:
pkgs.testers.nixosTest {
  name = "mesh-services-isolation";
  nodes.machine = { ... }: {
    imports = [
      self.nixosModules.mish-simulation
      self.nixosModules.mesh-services
    ];
    services.mesh-services = {
      supplyDrop = {
        enable = true;
        device = "/dev/serial/by-id/vm-dedicated-bbs";
        adminEnable = true;
      };
      reticulum.enable = true;
      retiBBS = {
        enable = true;
        webEnable = true;
      };
    };
    virtualisation.memorySize = 2048;
  };
  testScript = ''
    machine.start()
    machine.wait_for_unit("mish-control.service")
    machine.wait_for_unit("mesh-reticulum.service")
    machine.wait_for_unit("retibbs.service")
    machine.wait_for_unit("supply-drop.service")
    machine.wait_for_open_port(5000)
    machine.wait_for_open_port(8080)
    machine.succeed("curl -fsS http://127.0.0.1:5000/")
    machine.succeed("curl -fsS http://127.0.0.1:8080/")
    machine.succeed("test $(stat -c %U /var/lib/retibbs) = retibbs")
    machine.succeed("test $(stat -c %U /var/lib/mesh-reticulum) = mesh-reticulum")
    machine.succeed("test $(stat -c %U /var/lib/supply-drop) = supply-drop")
    identity = machine.succeed("sha256sum /var/lib/retibbs/server_identity.pem")
    machine.succeed("systemctl restart retibbs.service supply-drop.service")
    machine.wait_for_open_port(5000)
    machine.wait_for_open_port(8080)
    assert identity == machine.succeed("sha256sum /var/lib/retibbs/server_identity.pem")
    machine.succeed("systemctl stop retibbs.service mesh-reticulum.service supply-drop.service")
    machine.succeed("curl -fsS http://127.0.0.1:8787/health")
    machine.succeed("systemctl start mesh-reticulum.service retibbs.service supply-drop.service")
    machine.wait_for_open_port(5000)
    machine.wait_for_open_port(8080)
    assert identity == machine.succeed("sha256sum /var/lib/retibbs/server_identity.pem")
  '';
}
