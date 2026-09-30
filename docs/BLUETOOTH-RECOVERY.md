# Bluetooth PAN recovery

The Pi advertises a Bluetooth Classic Network Access Point (NAP). A Linux
computer acting as a PAN user (PANU) can connect and SSH to `10.77.0.1`
without Wi-Fi, Ethernet, or Internet access. This is not BLE tunneling and
does not use the hosted web remote. Modern macOS is not a supported client.

**Enroll and test the rescue computer before relying on this path.** The
computer needs a Bluetooth Classic-capable adapter, BlueZ, NetworkManager
with Bluetooth support, and a provisioned SSH key. Do not assume successful
BLE control or audio playback proves PAN connectivity.

## Prepare while the box is reachable

1. Provision the rescue computer's SSH public key through the existing
   provisioning flow. Confirm ordinary key-authenticated SSH works. Keep a
   record of the box's SSH host-key fingerprint; pairing does not replace
   SSH authentication or host verification.
2. On the Pi, record its Bluetooth controller address with `bluetoothctl
   list`. The supported boxes use USB Bluetooth adapters; this address is
   not the Wi-Fi MAC address or the hostname suffix.
3. Open the box's pairing window from its panel or existing web/BLE UI.
4. On the Linux rescue computer, run `bluetoothctl`, enable an agent, scan,
   and pair with the recorded Pi controller. For example, replacing the
   address below:

```text
power on
agent on
default-agent
scan on
pair AA:BB:CC:DD:EE:FF
trust AA:BB:CC:DD:EE:FF
scan off
quit
```

Confirm any pairing prompt. On the Pi, `bluetoothctl info <laptop-address>`
must report both `Paired: yes` and `Trusted: yes`. Boompi normally marks a
newly paired peer trusted automatically. If needed, explicitly trust that
known computer on the Pi while normal access still works. Trusting the Pi
on the laptop alone is not enough.

The recovery daemon does not own a pairing agent or enable pairing. An
already trusted peer can reconnect without boompid; enrolling a new peer
still requires the normal pairing flow or local administrative access.
All trusted Bluetooth peers can attempt SSH, but only provisioned SSH keys
can log in. This is not a separate recovery-device allowlist.

## Connect from Linux

Create a client profile on the laptop, substituting the Pi controller's
address. Do not run these commands on the Pi:

```sh
nmcli connection add type bluetooth con-name boompi-rescue \
  bluetooth.type panu bluetooth.bdaddr AA:BB:CC:DD:EE:FF \
  connection.autoconnect no \
  ipv4.method auto ipv4.never-default yes ipv4.ignore-auto-dns yes \
  ipv6.method disabled
nmcli connection up boompi-rescue
ssh -o HostKeyAlias=boompi-57fe root@10.77.0.1
```

Use the actual box ID for `HostKeyAlias`. Every box uses the same recovery
address, so the alias distinguishes their persistent host keys. Verify the
fingerprint recorded during enrollment on first connection; never bypass
host-key checking. Connect to only one Boompi recovery PAN at a time, and
ensure `10.77.0.0/24` does not conflict with another laptop network or VPN.

The Pi assigns addresses `10.77.0.10` through `10.77.0.50`. It advertises
neither a default gateway nor a DNS server. The laptop's existing Internet
connection should stay unchanged. The PAN does not provide Internet to the
Pi either; it provides local administrative access for repairs.

If DHCP fails but the Bluetooth link connects, a static client address can
still reach the Pi:

```sh
nmcli connection down boompi-rescue
nmcli connection modify boompi-rescue \
  ipv4.method manual ipv4.addresses 10.77.0.2/24 ipv4.gateway "" ipv4.dns ""
nmcli connection up boompi-rescue
ssh -o HostKeyAlias=boompi-57fe root@10.77.0.1
```

The static address is outside the DHCP pool. Restore DHCP later with
`nmcli connection modify boompi-rescue ipv4.method auto ipv4.addresses ""`.

## Service and isolation

- `boompi-pan-network.service` installs only PAN-owned nftables tables,
  then creates and addresses `br-pan`. Failure to install the firewall
  prevents NAP startup. It never bridges Wi-Fi or Ethernet into the PAN.
- `boompi-pan.service` keeps the BlueZ NAP registration alive on capable,
  powered adapters and handles BlueZ restarts and adapter replacement.
- `boompi-pan-dhcp.service` runs a separate DHCP-only dnsmasq instance.
  If it fails, the static-address fallback still works.
- NetworkManager leaves `br-pan` and `bnep*` strictly unmanaged. PAN does
  not depend on NetworkManager, boompid, the panel, or Wi-Fi association.
- The firewall permits IPv4 SSH to `10.77.0.1`, DHCP, and ping. It blocks
  other host services, IPv6, traffic forwarded to/from other networks,
  and traffic bridged between PAN clients. It does not modify global
  forwarding settings or NetworkManager's hotspot firewall tables.
- Bluetooth link security remains enabled. SSH remains key-only with
  the same persistent keys under `/data/ssh` as normal network access.

The bridge and scoped firewall remain when PAN services stop, so existing
BNEP devices cannot become unfiltered. Stopping `boompi-pan.service` removes
its NAP advertisement; restarting Bluetooth can start the service again.
For an administrative disable, mask `boompi-pan.service` rather than just
stopping it. Root-slot changes, including masks, need to be reapplied after
an OS image replacement unless incorporated into that image.

## Acceptance tests

Use a bench box with independent Ethernet or console recovery. **Do not
disconnect Wi-Fi, stop networking, or reboot a sealed box to test this
before proving its Bluetooth recovery path.**

1. Connect from the intended Linux laptop and SSH to `10.77.0.1`. Confirm
   the client uses its BNEP interface, and the Pi bridge has only BNEP ports.
2. Confirm the web UI (`:3001`) and SMB (`:445`) are unreachable over PAN,
   while SSH works and the laptop's default route and DNS are unchanged.
3. With that independent recovery path available, stop boompid on the Pi,
   disconnect and reconnect the laptop's PAN, and establish a fresh SSH
   session. Repeat with Pi Wi-Fi disabled and NetworkManager stopped.
4. Restore normal services. Restart Bluetooth, reconnect PAN, then reboot
   the Pi and check automatic NAP availability and a fresh SSH login.
5. Test on both Pi 3 and Pi 4, including the actual USB Bluetooth adapter.
   Keep the candidate off Nate's box until these tests pass there or on
   equivalent hardware with accessible recovery.

The automated tests use a private D-Bus with fake BlueZ for registration
lifecycle, and isolated Linux namespaces for the real firewall, bridge,
and DHCP configuration. They do not establish radio interoperability.

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p boompi-pan
sudo unshare --mount --net --pid --fork --mount-proc \
  python3 -B scripts/test-pan-network.py
```

## Troubleshooting and limits

On the Pi, use `systemctl status boompi-pan.service boompi-pan-network.service
boompi-pan-dhcp.service`, `journalctl -b -u boompi-pan.service`, `ip address
show br-pan`, and `nft list table inet boompi_pan`. On Linux, check
`nmcli device status`, the Bluetooth bond, and the PAN connection profile.

Opening pairing mode, nightly Bluetooth refresh, or USB-controller recovery
can interrupt a PAN session because audio and recovery share the adapter.
Reconnect using the saved Linux profile after the controller returns.

This cannot recover a kernel that fails to boot, dead Bluetooth hardware,
broken BlueZ, missing SSH keys/bonds, or unusable `/data`. A new OS image
must include the recovery service itself. Existing A/B health checks only
test the local daemon and do not guarantee recovery from network loss;
Bluetooth PAN supplements, rather than fixes, that limitation.
