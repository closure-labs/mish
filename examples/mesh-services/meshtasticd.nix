# Mish TCP adapter requires the separate event-connectivity change.
{ ... }: {
  services.meshtasticd = {
    enable = true;
    port = 4403;
    settings = {
      Lora.Module = "auto";
      General.MaxNodes = 200;
    };
  };
  services.mish.radio.meshtastic = {
    enable = true;
    tcpEndpoint = "127.0.0.1:4403";
  };
  # Do not open 4403 in the firewall. Configure and verify the actual hardware.
}
