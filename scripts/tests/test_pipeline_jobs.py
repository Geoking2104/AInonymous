import sys
import threading
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from pipeline_jobs import PipelineJobs, JobCancelled


class CancellationTests(unittest.TestCase):
    def test_cancel_during_forward_prevents_cache_commit_and_reuse(self):
        jobs = PipelineJobs()
        started, proceed = threading.Event(), threading.Event()
        result, cache = [], {}

        def forward():
            try:
                with jobs.work("job"):
                    started.set()
                    self.assertTrue(proceed.wait(2))
                    jobs.commit(lambda: cache.update(job="must not exist"))
            except JobCancelled as error:
                result.append(str(error))

        worker = threading.Thread(target=forward)
        worker.start()
        self.assertTrue(started.wait(2))
        jobs.cancel("job", "CH6 membership removed")
        proceed.set()
        worker.join(2)
        self.assertFalse(worker.is_alive())
        self.assertEqual(cache, {})
        self.assertEqual(result, ["CH6 membership removed"])
        self.assertEqual(jobs.failure("job"), "CH6 membership removed")
        with self.assertRaises(JobCancelled):
            with jobs.work("job"):
                self.fail("cancelled job resumed")
        with jobs.work("unrelated"):
            jobs.commit(lambda: cache.update(ok=True))
        self.assertEqual(cache, {"ok": True})


if __name__ == "__main__":
    unittest.main()
