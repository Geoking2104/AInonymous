import unittest

from run_sg1 import discovered_peers


RELAY = "http://127.0.0.1:4000"
KEYS = {"A": "key-a", "B": "key-b"}


def record(key, endpoint, dna="pilot", relay=RELAY):
    return {"agent_pub_key": key, "dna_hash": dna, "url": f"{relay}/{endpoint}"}


class DiscoveryTests(unittest.TestCase):
    def setUp(self):
        self.records = {"A": [record("key-a", "endpoint-a")],
                        "B": [record("key-b", "endpoint-b")]}

    def discovered(self):
        return discovered_peers(self.records, KEYS, RELAY)

    def test_requires_each_direction(self):
        self.assertEqual(self.discovered(), set())
        self.records["A"].append(record(None, "endpoint-b"))
        self.assertEqual(self.discovered(), {"A"})
        self.records["B"].append(record(None, "endpoint-a"))
        self.assertEqual(self.discovered(), {"A", "B"})

    def test_rejects_unrelated_remote_records(self):
        self.records["A"].extend([record(None, "third-endpoint"),
                                  record(None, "endpoint-b", dna="other"),
                                  record(None, "endpoint-b", relay=RELAY + "1")])
        self.assertEqual(self.discovered(), set())

    def test_rejects_ambiguous_local_address(self):
        self.records["B"] = [record("key-b", "endpoint-a"), record(None, "endpoint-a")]
        self.records["A"].append(record(None, "endpoint-a"))
        self.assertEqual(self.discovered(), set())

    def test_rejects_matching_records_on_another_relay(self):
        other = RELAY + "1"
        self.records = {"A": [record("key-a", "a", relay=other), record(None, "b", relay=other)],
                        "B": [record("key-b", "b", relay=other), record(None, "a", relay=other)]}
        self.assertEqual(self.discovered(), set())

    def test_requires_local_key_and_same_dna(self):
        self.records["A"].append(record(None, "endpoint-b", dna="other"))
        for key, dna in [("key-b", "other"), ("unknown-key", "pilot")]:
            with self.subTest(key=key, dna=dna):
                self.records["B"] = [record(key, "endpoint-b", dna=dna), record(None, "endpoint-a")]
                self.assertEqual(self.discovered(), set())
