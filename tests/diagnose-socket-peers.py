"""Live socket checks; run on the host where pidfd_getfd is permitted."""

import importlib.util
import os
from pathlib import Path
import socket
import unittest


SPEC = importlib.util.spec_from_file_location(
    "diagnosis", Path(__file__).resolve().parents[1] / "scripts/diagnose_socket_peers.py")
diagnosis = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(diagnosis)


class SocketDiagnosisTests(unittest.TestCase):
    def target(self, sock):
        metadata = os.fstat(sock.fileno())
        return {"pid": os.getpid(), "fd": sock.fileno(),
                "start_time_ticks": diagnosis.start_ticks(os.getpid()),
                "inode": metadata.st_ino, "device": metadata.st_dev}

    def test_unix_credentials_data_and_descriptor_lifetime(self):
        left, right = socket.socketpair()
        with left, right:
            right.sendall(b"still queued")
            left.settimeout(1)
            target = self.target(left)
            fd_count = len(os.listdir("/proc/self/fd"))
            for _ in range(3):
                result = diagnosis.inspect_target(target)
                self.assertEqual(result["classification"], "unnamed_unix_peer")
                self.assertEqual(result["peer_credentials"]["pid"], os.getpid())
            self.assertEqual(len(os.listdir("/proc/self/fd")), fd_count)
            self.assertEqual(left.gettimeout(), 1)
            self.assertEqual(left.recv(12), b"still queued")
            for key in ("inode", "device", "start_time_ticks"):
                with self.assertRaises(OSError):
                    diagnosis.inspect_target({**target, key: target[key] + 1})
            left.close()
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as replacement:
                os.dup2(replacement.fileno(), target["fd"])
                try:
                    with self.assertRaises(OSError):
                        diagnosis.inspect_target(target)
                finally:
                    if replacement.fileno() != target["fd"]:
                        os.close(target["fd"])

    def test_tcp_listener_and_connected_endpoints(self):
        for family, loopback in ((socket.AF_INET, "127.0.0.1"), (socket.AF_INET6, "::1")):
            with self.subTest(family=family), socket.socket(family) as listener:
                listener.bind((loopback, 0))
                listener.listen()
                result = diagnosis.inspect_target(self.target(listener))
                self.assertEqual(result["classification"], "listener")
                self.assertEqual(result["local"]["port"], listener.getsockname()[1])
                with socket.socket(family) as client:
                    client.connect(listener.getsockname())
                    accepted, _ = listener.accept()
                    with accepted:
                        result = diagnosis.inspect_target(self.target(client))
                        self.assertEqual(result["classification"], "peer_address_found")
                        self.assertEqual(result["peer"]["ip"], loopback)
                        self.assertEqual(result["peer"]["port"], listener.getsockname()[1])

    def test_udp_and_netlink_default_peer(self):
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as udp:
            result = diagnosis.inspect_target(self.target(udp))
            self.assertEqual(result["classification"], "unconnected")
            udp.connect(("127.0.0.1", 54321))
            result = diagnosis.inspect_target(self.target(udp))
            self.assertEqual(result["peer"]["port"], 54321)
        with socket.socket(socket.AF_NETLINK, socket.SOCK_RAW, 0) as netlink:
            netlink.bind((0, 0))
            result = diagnosis.inspect_target(self.target(netlink))
            self.assertEqual(result["classification"], "netlink_default_peer")
            self.assertEqual(result["peer"]["endpoint"], "kernel")
            self.assertEqual(result["local"]["port_id"], netlink.getsockname()[0])

    def test_abstract_unix_name_preserves_embedded_nulls(self):
        with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as server:
            name = b"\0procinsh-diagnosis-" + str(os.getpid()).encode() + b"\0tail"
            server.bind(name)
            with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as client:
                client.connect(name)
                result = diagnosis.inspect_target(self.target(client))
                self.assertEqual(result["peer"]["kind"], "abstract")
                self.assertEqual(result["peer"]["name_hex"], name[1:].hex())


if __name__ == "__main__":
    unittest.main()
