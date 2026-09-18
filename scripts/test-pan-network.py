#!/usr/bin/env python3
"""Linux-only functional tests of the shipped PAN network script and configs.

Run as root (also safe inside CI's outer unshare):
  sudo unshare --mount --net --pid --fork --mount-proc \
    python3 -B scripts/test-pan-network.py

Dependencies: Python 3, nftables, iproute2, dnsmasq, util-linux (unshare/mount),
and procps (sysctl). No Internet, Bluetooth device, pip package, or sshd needed.
The entry point ALWAYS creates another mount/network/PID namespace. The worker
verifies inherited namespace descriptors before mounting private /etc, /run,
/tmp, /var and network-namespace-specific sysfs. Nothing is written to the checkout.
"""

import fcntl
import ipaddress
import json
import os
from pathlib import Path
import re
import secrets
import selectors
import shutil
import signal
import socket
import struct
import subprocess
import sys
import time
import traceback


SCRIPT = Path(__file__).resolve()
OVERLAY = SCRIPT.parents[1] / "buildroot/board/boompi/rootfs-overlay"
NETWORK = OVERLAY / "usr/bin/boompi-pan-network"
TOKEN = b"pan-test-listener\n"
NAMESPACES = ("pan-client-a", "pan-client-b", "pan-uplink")


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def interrupted(signum, frame):
    raise KeyboardInterrupt(f"Signal {signum}")


def run(*args, ok=True, timeout=15, **kwargs):
    result = subprocess.run(
        [str(arg) for arg in args], capture_output=True, text=True,
        timeout=timeout, **kwargs,
    )
    if ok and result.returncode:
        raise AssertionError(
            f"Command failed ({result.returncode}): {' '.join(map(str, args))}\n"
            f"{result.stdout}{result.stderr}"
        )
    return result


def ip(namespace, *args):
    return run("ip", *(["-n", namespace] if namespace else []), *args)


def helper_command(namespace, *args):
    return (["ip", "netns", "exec", namespace] if namespace else []) + [
        sys.executable, "-B", str(SCRIPT), "--helper", *map(str, args),
    ]


def probe(namespace, *args):
    return run(*helper_command(namespace, *args), timeout=30).stdout.strip()


def checksum(data):
    if len(data) % 2:
        data += b"\0"
    total = sum(struct.unpack(f"!{len(data) // 2}H", data))
    while total >> 16:
        total = (total & 0xFFFF) + (total >> 16)
    return (~total) & 0xFFFF


def mac_bytes(address):
    return bytes.fromhex(address.replace(":", ""))


def dhcp(interface, forbidden=False, destination="255.255.255.255",
         destination_mac="ff:ff:ff:ff:ff:ff"):
    """Use Ethernet directly, so DISCOVER works before any IP is configured."""
    with socket.socket(socket.AF_PACKET, socket.SOCK_RAW, socket.htons(3)) as sock:
        sock.bind((interface, 3))
        mac = fcntl.ioctl(
            sock.fileno(), 0x8927, struct.pack("256s", interface.encode())
        )[18:24]
        xid = secrets.randbits(32)

        def exchange(message_type, extra, expected):
            bootp = struct.pack(
                "!BBBBIHH4s4s4s4s16s64s128s", 1, 1, 6, 0, xid, 0, 0x8000,
                bytes(4), bytes(4), bytes(4), bytes(4), mac.ljust(16, b"\0"),
                bytes(64), bytes(128),
            )
            payload = (bootp + b"\x63\x82\x53\x63" + bytes([53, 1, message_type])
                       + b"\x37\x04\x01\x03\x06\x33" + extra + b"\xff")
            payload = payload.ljust(300, b"\0")
            udp = struct.pack("!HHHH", 68, 67, 8 + len(payload), 0) + payload
            header = struct.pack(
                "!BBHHHBBH4s4s", 0x45, 0, 20 + len(udp), 0, 0, 64, 17, 0,
                bytes(4), socket.inet_aton(destination),
            )
            header = header[:10] + struct.pack("!H", checksum(header)) + header[12:]
            frame = mac_bytes(destination_mac) + mac + b"\x08\x00" + header + udp
            for _ in range(2 if forbidden else 3):
                sock.send(frame)
                # dnsmasq may spend several seconds checking a new lease for
                # address conflicts before sending its first OFFER.
                deadline = time.monotonic() + (1 if forbidden else 4)
                while time.monotonic() < deadline:
                    sock.settimeout(max(0.001, deadline - time.monotonic()))
                    try:
                        packet = sock.recv(65535)
                    except socket.timeout:
                        break
                    if len(packet) < 42 or packet[12:14] != b"\x08\x00":
                        continue
                    ihl = (packet[14] & 15) * 4
                    start = 14 + ihl
                    if packet[23] != 17 or len(packet) < start + 8 + 240:
                        continue
                    if struct.unpack("!HH", packet[start:start + 4]) != (67, 68):
                        continue
                    reply = packet[start + 8:]
                    if reply[0] != 2 or reply[4:8] != struct.pack("!I", xid):
                        continue
                    if reply[28:34] != mac or reply[236:240] != b"\x63\x82\x53\x63":
                        continue
                    check(not forbidden, f"DHCP responded on forbidden interface {interface}")
                    options = {}
                    pos = 240
                    while pos < len(reply):
                        code = reply[pos]
                        pos += 1
                        if code == 255:
                            break
                        if code == 0:
                            continue
                        check(pos < len(reply), "Truncated DHCP option length")
                        length = reply[pos]
                        pos += 1
                        check(pos + length <= len(reply), "Truncated DHCP option")
                        options[code] = reply[pos:pos + length]
                        pos += length
                    if expected == 5 and options.get(53) == b"\x02":
                        continue  # A delayed OFFER from a retransmitted DISCOVER.
                    check(options.get(53) == bytes([expected]), f"Unexpected DHCP type: {options}")
                    check(3 not in options, "DHCP advertised a default router")
                    check(6 not in options, "DHCP advertised a DNS server")
                    check(options.get(1) == socket.inet_aton("255.255.255.0"), "Wrong DHCP mask")
                    check(options.get(54) == socket.inet_aton("10.77.0.1"), "Wrong DHCP server")
                    check(len(options.get(51, b"")) == 4, "Missing DHCP lease time")
                    check(struct.unpack("!I", options[51])[0] > 0, "Zero DHCP lease time")
                    address = socket.inet_ntoa(reply[16:20])
                    check(ipaddress.IPv4Address("10.77.0.10") <= ipaddress.IPv4Address(address)
                          <= ipaddress.IPv4Address("10.77.0.50"), f"Out-of-pool lease: {address}")
                    return address, options[54]
            check(forbidden, f"No DHCP response to message {message_type} on {interface}")

        offer = exchange(1, b"", 2)
        if forbidden:
            print("No DHCP response", flush=True)
            return
        address, server = offer
        ack, _ = exchange(3, b"\x32\x04" + socket.inet_aton(address) + b"\x36\x04" + server, 5)
        check(ack == address, "ACK changed the offered address")
        print(json.dumps({"address": address}), flush=True)


def tcp_probe(address, port, allowed):
    family = socket.AF_INET6 if ":" in address else socket.AF_INET
    with socket.socket(family, socket.SOCK_STREAM) as sock:
        sock.settimeout(0.8)
        try:
            sock.connect((address, int(port)))
        except (TimeoutError, ConnectionRefusedError, OSError) as error:
            if allowed:
                raise AssertionError(f"TCP {address}:{port} should be reachable: {error}") from error
            # A missing route is a fixture failure, not evidence of filtering.
            check(isinstance(error, (TimeoutError, ConnectionRefusedError))
                  or error.errno in (13, 113), f"Invalid negative TCP probe: {error}")
            return
        check(allowed, f"TCP {address}:{port} unexpectedly accepted a connection")
        received = b""
        while len(received) < len(TOKEN):
            chunk = sock.recv(len(TOKEN) - len(received))
            check(chunk, "Listener closed without its test token")
            received += chunk
        check(received == TOKEN, "Connected to an unexpected listener")


def icmp_probe(address, allowed):
    ipv6 = ":" in address
    family = socket.AF_INET6 if ipv6 else socket.AF_INET
    protocol = socket.IPPROTO_ICMPV6 if ipv6 else socket.IPPROTO_ICMP
    identifier = secrets.randbelow(65536)
    payload = b"boompi-pan-icmp-" + secrets.token_bytes(8)
    packet = struct.pack("!BBHHH", 128 if ipv6 else 8, 0, 0, identifier, 1) + payload
    if not ipv6:
        packet = packet[:2] + struct.pack("!H", checksum(packet)) + packet[4:]
    with socket.socket(family, socket.SOCK_RAW, protocol) as sock:
        sock.sendto(packet, (address, 0))
        deadline = time.monotonic() + 1
        while time.monotonic() < deadline:
            sock.settimeout(max(0.001, deadline - time.monotonic()))
            try:
                reply, source = sock.recvfrom(4096)
            except socket.timeout:
                break
            if not ipv6:
                reply = reply[(reply[0] & 15) * 4:]
            if (source[0] == address and len(reply) >= 8
                    and reply[0] == (129 if ipv6 else 0)
                    and reply[4:8] == struct.pack("!HH", identifier, 1)
                    and reply[8:] == payload):
                check(allowed, f"ICMP {address} unexpectedly passed")
                return
        check(not allowed, f"ICMP {address} should be reachable")


def listen():
    selector = selectors.DefaultSelector()
    for family, address in ((socket.AF_INET, "0.0.0.0"), (socket.AF_INET6, "::")):
        for port in (22, 3001):
            sock = socket.socket(family, socket.SOCK_STREAM)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            if family == socket.AF_INET6:
                sock.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
            sock.bind((address, port))
            sock.listen(16)
            selector.register(sock, selectors.EVENT_READ)
    print("READY", flush=True)
    while True:
        for key, _ in selector.select():
            connection, _ = key.fileobj.accept()
            with connection:
                connection.settimeout(1)
                try:
                    connection.sendall(TOKEN)
                except OSError:
                    pass


def helper(args):
    check(Path("/run/pan-test-pidns").read_text() == os.readlink("/proc/self/ns/pid")
          and b"--worker" in Path("/proc/1/cmdline").read_bytes().split(b"\0"),
          "Helpers may only run inside the isolated test worker")
    if args[0] == "listen":
        listen()
    elif args[0] == "dhcp":
        dhcp(args[1], args[2] == "forbidden", *args[3:])
    elif args[0] == "tcp":
        tcp_probe(args[1], args[2], args[3] == "allowed")
    elif args[0] == "icmp":
        icmp_probe(args[1], args[2] == "allowed")
    else:
        raise ValueError(f"Unknown helper: {args}")


def private_mounts(descriptors):
    check(os.geteuid() == 0 and os.getpid() == 1, "Worker requires a fresh root PID namespace")
    check(len(descriptors) == 3, "Missing original namespace descriptors")
    for kind, descriptor in zip(("mnt", "net", "pid"), descriptors):
        descriptor = int(descriptor)
        check(os.readlink(f"/proc/self/fd/{descriptor}").startswith(f"{kind}:["),
              f"Not a {kind} namespace descriptor")
        original = os.fstat(descriptor)
        current = os.stat(f"/proc/self/ns/{kind}")
        check((original.st_dev, original.st_ino) != (current.st_dev, current.st_ino),
              f"Refusing to run in the caller's {kind} namespace")
        check(os.stat(f"/proc/1/ns/{kind}").st_ino == current.st_ino,
              "procfs does not describe this PID namespace")
        os.close(descriptor)
    # This check happens before nft or any network mutation.
    links = json.loads(run("ip", "-j", "link", "show").stdout)
    check([link["ifname"] for link in links] == ["lo"], "Network namespace is not empty")
    run("mount", "--make-rprivate", "/")
    check(not run("nft", "list", "ruleset").stdout.strip(), "Network namespace has existing firewall rules")

    # Read the small fixture into memory before hiding the caller's /etc.
    etc = {}
    for name in ("passwd", "group", "nsswitch.conf", "hosts", "services", "ld.so.cache"):
        source = Path("/etc") / name
        if source.is_file():
            etc[name] = source.read_bytes()
    for name in ("pan.nft", "pan-dnsmasq.conf"):
        etc[f"boompi/{name}"] = (OVERLAY / "etc/boompi" / name).read_bytes()
    for target in ("/etc", "/run", "/tmp", "/var"):
        run("mount", "-t", "tmpfs", "-o", "nosuid,nodev,mode=755", "pan-test", target)
    os.chmod("/tmp", 0o1777)
    Path("/var/run").symlink_to("/run")
    for directory in ("/var/lib/misc", "/var/lib/dnsmasq", "/var/log"):
        Path(directory).mkdir(parents=True, exist_ok=True)
    for name, content in etc.items():
        target = Path("/etc") / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
    run("mount", "-t", "sysfs", "-o", "nosuid,nodev,noexec", "sysfs", "/sys")
    check({entry.name for entry in Path("/sys/class/net").iterdir()} == {"lo"},
          "sysfs does not match the private network namespace")
    Path("/run/pan-test-pidns").write_text(os.readlink("/proc/self/ns/pid"))
    ip(None, "link", "set", "lo", "up")


def network(expected_error=None):
    result = run("/bin/sh", NETWORK, ok=expected_error is None)
    if expected_error is not None:
        check(result.returncode != 0, "Network setup unexpectedly succeeded")
        check(expected_error in result.stderr, f"Wrong setup failure: {result.stderr}")


def bridge_state():
    links = json.loads(ip(None, "-j", "-d", "link", "show", "br-pan").stdout)
    addresses = json.loads(ip(None, "-j", "-4", "address", "show", "br-pan").stdout)
    return links, addresses


def ruleset():
    return run("nft", "list", "ruleset").stdout


def test_setup():
    run("nft", "-f", "-", input='table inet pan_test_keep { chain sentinel { counter comment "keep me"; } }\n')
    keep = run("nft", "list", "table", "inet", "pan_test_keep").stdout
    for kind, owner, error in (
        ("dummy", "boompi-pan", "br-pan is not a bridge"),
        ("bridge", "someone-else", "br-pan is not owned"),
        ("bridge", "", "br-pan is not owned"),
    ):
        ip(None, "link", "add", "br-pan", "type", kind)
        if owner:
            ip(None, "link", "set", "br-pan", "alias", owner)
        ip(None, "addr", "add", "192.0.2.1/24", "dev", "br-pan")
        before, firewall = bridge_state(), ruleset()
        network(error)
        check(bridge_state() == before and ruleset() == firewall, "Ownership refusal mutated resources")
        ip(None, "link", "del", "br-pan")
    print("PASS ownership refusal (non-bridge, foreign owner, unowned bridge)", flush=True)

    ip(None, "link", "add", "br-pan", "alias", "boompi-pan", "type", "bridge")
    ip(None, "link", "add", "foreign0", "type", "dummy")
    ip(None, "link", "set", "foreign0", "master", "br-pan")
    before, firewall = bridge_state(), ruleset()
    member = ip(None, "-j", "-d", "link", "show", "foreign0").stdout
    network("unexpected bridge member")
    check(bridge_state() == before and ruleset() == firewall, "Foreign-member refusal mutated bridge/firewall")
    check(ip(None, "-j", "-d", "link", "show", "foreign0").stdout == member, "Foreign member was changed")
    ip(None, "link", "del", "foreign0")
    ip(None, "link", "del", "br-pan")
    print("PASS foreign bridge member refusal", flush=True)

    config = Path("/etc/boompi/pan.nft")
    original = config.read_bytes()
    invalid_suffixes = (
        b"\nthis is deliberately invalid nft syntax !!!\n",
        b"\nadd rule bridge boompi_pan missing_chain counter\n",
    )
    for invalid in invalid_suffixes:
        config.write_bytes(original + invalid)
        try:
            before = ruleset()
            network("Error")
            check(not Path("/sys/class/net/br-pan").exists(), "Invalid nft config created a bridge")
            check(ruleset() == before, "Invalid initial nft transaction changed the firewall")
        finally:
            config.write_bytes(original)
    network()
    # Let any automatic link-local DAD finish before comparing link state.
    time.sleep(1.2)
    before, firewall = bridge_state(), ruleset()
    network()
    network()
    check(bridge_state() == before and ruleset() == firewall, "Repeated setup was not idempotent")
    check(Path("/sys/class/net/br-pan/ifalias").read_text().strip() == "boompi-pan", "Wrong bridge owner")
    addresses = before[1][0]["addr_info"]
    check(any(a["local"] == "10.77.0.1" and a["prefixlen"] == 24 for a in addresses), "Missing PAN address")
    for invalid in invalid_suffixes:
        config.write_bytes(original + invalid)
        try:
            network("Error")
            check(bridge_state() == before and ruleset() == firewall, "Failed reload did not preserve live protection")
        finally:
            config.write_bytes(original)
    check(run("nft", "list", "table", "inet", "pan_test_keep").stdout == keep, "Unrelated nft table was changed")
    print("PASS idempotence, atomic fail-closed setup/reload, unrelated nft table preserved", flush=True)
    return keep


def add_peer(namespace, port, bridge=False):
    run("ip", "netns", "add", namespace)
    ip(None, "link", "add", port, "type", "veth", "peer", "name", "eth0", "netns", namespace)
    if bridge:
        ip(None, "link", "set", port, "master", "br-pan")
    ip(None, "link", "set", port, "up")
    ip(namespace, "link", "set", "lo", "up")
    ip(namespace, "link", "set", "eth0", "up")


def link_mac(namespace, interface):
    return json.loads(ip(namespace, "-j", "link", "show", interface).stdout)[0]["address"]


def neighbor(namespace, address, mac, interface="eth0"):
    ip(namespace, "neigh", "replace", address, "lladdr", mac, "nud", "permanent", "dev", interface)


def start_listener(namespace, processes):
    process = subprocess.Popen(helper_command(namespace, "listen"), stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True)
    processes.append(process)
    with selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        check(selector.select(5), f"Listener in {namespace or 'host'} did not start")
    line = process.stdout.readline()
    check(line.strip() == "READY", f"Listener in {namespace or 'host'} failed: {line}")


def traffic_tests(processes, keep):
    a, b, uplink = NAMESPACES
    add_peer(a, "bnep0", bridge=True)
    add_peer(b, "bnep1", bridge=True)
    add_peer(uplink, "uplink0")
    ip(None, "addr", "add", "198.18.0.1/24", "dev", "uplink0")
    # Make the uplink eligible for the pool without stealing the bridge route.
    # DHCP silence must come from interface scoping, not a missing lease range.
    ip(None, "addr", "add", "10.77.0.254/24", "dev", "uplink0", "noprefixroute")
    ip(uplink, "addr", "add", "198.18.0.2/24", "dev", "eth0")
    ip(None, "route", "add", "default", "via", "198.18.0.2")
    ip(uplink, "route", "add", "10.77.0.0/24", "via", "198.18.0.1")
    run("sysctl", "-qw", "net.ipv4.ip_forward=1", "net.ipv6.conf.all.forwarding=1")
    for ns, dev, address in (
        (None, "br-pan", "fd77:77::1/64"), (a, "eth0", "fd77:77::10/64"),
        (b, "eth0", "fd77:77::11/64"), (None, "uplink0", "fd77:88::1/64"),
        (uplink, "eth0", "fd77:88::2/64"),
    ):
        ip(ns, "-6", "addr", "add", address, "dev", dev, "nodad")
    ip(uplink, "-6", "route", "add", "fd77:77::/64", "via", "fd77:88::1")
    time.sleep(1.2)  # Allow automatic link-local DAD before the uplink snapshot.
    before_routes = ip(None, "-j", "route", "show", "table", "all").stdout
    before_uplink = ip(None, "-j", "addr", "show", "uplink0").stdout
    network()
    check(ip(None, "-j", "route", "show", "table", "all").stdout == before_routes, "Setup changed routes")
    check(ip(None, "-j", "addr", "show", "uplink0").stdout == before_uplink, "Setup changed uplink")
    check(run("sysctl", "-n", "net.ipv4.ip_forward").stdout.strip() == "1", "Setup changed forwarding")
    print("PASS repeated setup accepts bnep members and preserves uplink/routes/forwarding", flush=True)

    log = open("/tmp/pan-dnsmasq.log", "w+")
    try:
        dns = subprocess.Popen([
            "dnsmasq", "--keep-in-foreground", "--conf-file=/etc/boompi/pan-dnsmasq.conf",
        ], stdout=log, stderr=subprocess.STDOUT)
        processes.append(dns)
        deadline = time.monotonic() + 5
        while True:
            check(dns.poll() is None, "dnsmasq exited during startup")
            listeners = run("ss", "-H", "-luntp").stdout
            owned = [line for line in listeners.splitlines() if f"pid={dns.pid}," in line]
            if owned:
                break
            check(time.monotonic() < deadline, "dnsmasq did not open its DHCP socket")
            time.sleep(0.1)
        check(all(line.startswith("udp") and re.search(r":67\s", line) for line in owned),
              f"dnsmasq opened a non-DHCP listener: {owned}")
        addresses = {}
        for ns in (a, b):
            addresses[ns] = json.loads(probe(ns, "dhcp", "eth0", "allowed"))["address"]
            ip(ns, "addr", "add", f"{addresses[ns]}/24", "dev", "eth0")
        check(addresses[a] != addresses[b], "DHCP assigned duplicate leases")
        probe(uplink, "dhcp", "eth0", "forbidden")
        probe(uplink, "dhcp", "eth0", "forbidden", "198.18.0.1", link_mac(None, "uplink0"))
        check(dns.poll() is None, "dnsmasq died during DHCP probes")
        probe(a, "dhcp", "eth0", "allowed")
        print("PASS real DHCP DISCOVER/OFFER/REQUEST/ACK, pool/mask, no router/DNS options; DHCP absent on uplink", flush=True)

        # Static neighbors ensure negative IPv6/bridge probes exercise filtering
        # of the actual payload, not merely blocked ARP or neighbor discovery.
        for ns, other, ipv6, other_ipv6 in ((a, b, "fd77:77::10", "fd77:77::11"),
                                          (b, a, "fd77:77::11", "fd77:77::10")):
            ip(ns, "route", "add", "198.18.0.0/24", "via", "10.77.0.1")
            ip(ns, "-6", "route", "add", "fd77:88::/64", "via", "fd77:77::1")
            for address in ("10.77.0.1", "fd77:77::1"):
                neighbor(ns, address, link_mac(None, "br-pan"))
            for address in (addresses[other], other_ipv6):
                neighbor(ns, address, link_mac(other, "eth0"))
            for address in (addresses[ns], ipv6):
                neighbor(None, address, link_mac(ns, "eth0"), "br-pan")
        for ns in (None, a, b, uplink):
            start_listener(ns, processes)
            probe(ns, "tcp", "127.0.0.1", "22", "allowed")
            probe(ns, "tcp", "127.0.0.1", "3001", "allowed")
            probe(ns, "tcp", "::1", "22", "allowed")
            probe(ns, "tcp", "::1", "3001", "allowed")

        blocked = []
        for ns in (a, b):
            probe(ns, "tcp", "10.77.0.1", "22", "allowed")
            probe(ns, "icmp", "10.77.0.1", "allowed")
            blocked.extend([
                (ns, "tcp", "10.77.0.1", "3001"),
                (ns, "tcp", "198.18.0.1", "22"),
                (ns, "tcp", "fd77:77::1", "22"),
                (ns, "tcp", "fd77:77::1", "3001"),
                (ns, "icmp", "fd77:77::1"),
                (ns, "tcp", "198.18.0.2", "22"),
                (ns, "icmp", "198.18.0.2"),
                (ns, "tcp", "fd77:88::2", "22"),
                (ns, "icmp", "fd77:88::2"),
                (uplink, "tcp", addresses[ns], "22"),
                (uplink, "icmp", addresses[ns]),
            ])
        for ns, other, ipv6 in ((a, b, "fd77:77::11"), (b, a, "fd77:77::10")):
            blocked.extend([
                (ns, "tcp", addresses[other], "22"),
                (ns, "icmp", addresses[other]),
                (ns, "tcp", ipv6, "22"),
                (ns, "icmp", ipv6),
                (uplink, "tcp", ipv6, "22"),
                (uplink, "icmp", ipv6),
            ])
        for ns, *args in blocked:
            probe(ns, *args, "blocked")
        print("PASS IPv4 TCP/22 and ICMP allowed; TCP/3001 and IPv6 host access blocked", flush=True)
        print("PASS IPv4/IPv6 routed uplink traffic and client bridging blocked in both directions", flush=True)

        # These are fixture-only positive controls: every denied path must work
        # when just the two PAN tables are absent. Never flush the ruleset.
        run("nft", "delete", "table", "inet", "boompi_pan")
        run("nft", "delete", "table", "bridge", "boompi_pan")
        for ns, *args in blocked:
            probe(ns, *args, "allowed")
        probe(uplink, "dhcp", "eth0", "forbidden")
        probe(uplink, "dhcp", "eth0", "forbidden", "198.18.0.1", link_mac(None, "uplink0"))
        probe(a, "dhcp", "eth0", "allowed")
        network()
        probe(a, "tcp", "10.77.0.1", "22", "allowed")
        probe(a, "tcp", "10.77.0.1", "3001", "blocked")
        check(run("nft", "list", "table", "inet", "pan_test_keep").stdout == keep, "Unrelated nft table changed")
        check(all(process.poll() is None for process in processes), "A test daemon exited")
        print("PASS all denied paths reachable without PAN firewall; DHCP still bridge-only; protection restored", flush=True)
    finally:
        log.flush()
        log.seek(0)
        print("--- dnsmasq log ---\n" + log.read(), flush=True)
        log.close()


def worker(descriptors):
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    os.environ["LC_ALL"] = "C"
    private_mounts(descriptors)
    processes = []
    try:
        keep = test_setup()
        traffic_tests(processes, keep)
        print("PASS all PAN network functional tests", flush=True)
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
        for process in reversed(processes):
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=3)
            if process.stdout:
                process.stdout.close()
        for namespace in NAMESPACES:
            run("ip", "netns", "del", namespace, ok=False)
        # Namespace PID 1 exiting also kills orphaned children. All mounts,
        # links, routes and firewall tables disappear with these namespaces.


def main():
    if sys.platform != "linux":
        raise SystemExit("PAN functional tests require Linux; no changes made")
    if len(sys.argv) > 1:
        if sys.argv[1] == "--helper":
            helper(sys.argv[2:])
            return 0
        if sys.argv[1] == "--worker":
            worker(sys.argv[2:])
            return 0
        raise SystemExit("Usage: sudo python3 -B scripts/test-pan-network.py")
    check(os.geteuid() == 0, "Run as root; the harness creates its own isolated namespaces")
    for executable in ("unshare", "mount", "ip", "ss", "nft", "dnsmasq", "sysctl"):
        check(shutil.which(executable), f"Missing dependency: {executable}")
    signal.signal(signal.SIGTERM, interrupted)
    descriptors = [os.open(f"/proc/self/ns/{kind}", os.O_RDONLY) for kind in ("mnt", "net", "pid")]
    try:
        process = subprocess.Popen([
            "unshare", "--mount", "--net", "--pid", "--fork", "--mount-proc", "--kill-child=TERM",
            sys.executable, "-B", str(SCRIPT), "--worker", *map(str, descriptors),
        ], pass_fds=descriptors, start_new_session=True)
        try:
            return process.wait(timeout=240)
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
    finally:
        for descriptor in descriptors:
            os.close(descriptor)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (AssertionError, OSError, subprocess.SubprocessError, KeyboardInterrupt):
        traceback.print_exc()
        sys.exit(1)
