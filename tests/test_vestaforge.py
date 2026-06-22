from __future__ import annotations

import os
import tempfile
import unittest

from vestaforge.storage import VestaForgeStore, default_config


class StoreTests(unittest.TestCase):
    def make_store(self) -> VestaForgeStore:
        self.tempdir = tempfile.TemporaryDirectory()
        config = default_config(self.tempdir.name)
        config["nostr"]["relays"] = ["mock://unit"]
        return VestaForgeStore(self.tempdir.name, config)

    def tearDown(self) -> None:
        if hasattr(self, "tempdir"):
            self.tempdir.cleanup()

    def test_simulated_waypoint_is_idempotent_by_id(self) -> None:
        store = self.make_store()
        first, first_created = store.add_simulated_waypoint(
            waypoint_id="wp-idempotent",
            lat=44.0,
            lon=-123.0,
            label="First",
            visibility="public",
        )
        second, second_created = store.add_simulated_waypoint(
            waypoint_id="wp-idempotent",
            lat=45.0,
            lon=-124.0,
            label="Second",
            visibility="private",
        )
        self.assertTrue(first_created)
        self.assertFalse(second_created)
        self.assertEqual(first, second)
        self.assertEqual(second["position"]["lat"], 44.0)
        self.assertEqual(second["visibility"], "public")

    def test_private_geopack_cannot_publish(self) -> None:
        store = self.make_store()
        store.add_simulated_waypoint(
            waypoint_id="wp-private",
            lat=44.0,
            lon=-123.0,
            visibility="private",
        )
        store.create_geopack("Private Pack", ["wp-private"], visibility="private", pack_id="pack-private")
        with self.assertRaisesRegex(ValueError, "publication.visibility must be public"):
            store.publish_geopack("pack-private")

    def test_public_geopack_requires_public_waypoints(self) -> None:
        store = self.make_store()
        store.add_simulated_waypoint(
            waypoint_id="wp-friends",
            lat=44.0,
            lon=-123.0,
            visibility="friends",
        )
        with self.assertRaisesRegex(ValueError, "public GeoPacks may only include public waypoints"):
            store.create_geopack("Bad Public Pack", ["wp-friends"], visibility="public")

    def test_public_geopack_publish_writes_mock_publication(self) -> None:
        store = self.make_store()
        store.add_simulated_waypoint(
            waypoint_id="wp-public",
            lat=44.0,
            lon=-123.0,
            label="Public",
            visibility="public",
        )
        store.create_geopack("Public Pack", ["wp-public"], visibility="public", pack_id="pack-public")
        result = store.publish_geopack("pack-public")
        self.assertEqual(result["relays"][0]["status"], "stored")
        self.assertTrue((store.publications_dir / "pack-public.json").exists())

    def test_geopack_content_hash_is_stable_for_identical_inputs(self) -> None:
        os.environ["VF_FIXED_TIME"] = "2026-06-22T00:00:00Z"
        try:
            store_a = self.make_store()
            store_a.add_simulated_waypoint(
                waypoint_id="wp-stable",
                lat=44.0,
                lon=-123.0,
                label="Stable",
                visibility="public",
            )
            pack_a, _ = store_a.create_geopack("Stable Pack", ["wp-stable"], visibility="public")
            self.tempdir.cleanup()

            store_b = self.make_store()
            store_b.add_simulated_waypoint(
                waypoint_id="wp-stable",
                lat=44.0,
                lon=-123.0,
                label="Stable",
                visibility="public",
            )
            pack_b, _ = store_b.create_geopack("Stable Pack", ["wp-stable"], visibility="public")
            self.assertEqual(pack_a["contentHash"], pack_b["contentHash"])
        finally:
            os.environ.pop("VF_FIXED_TIME", None)

    def test_operator_bootstrap_is_persistent_and_requires_mfa(self) -> None:
        store = self.make_store()
        first, created = store.bootstrap_operator()
        second, created_again = store.bootstrap_operator()
        self.assertTrue(created)
        self.assertFalse(created_again)
        self.assertEqual(first, second)
        self.assertTrue(second["mfaRequired"])


if __name__ == "__main__":
    unittest.main()
