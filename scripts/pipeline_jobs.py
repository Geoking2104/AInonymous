"""Cooperative per-job cancellation, independent of torch for deterministic tests.

The model lock preserves the previous serial inference behavior while FastAPI's
thread pool leaves its event loop free to receive cancellation during a forward.
A running GPU kernel is not preemptible here; cancellation is checked per layer.
"""
from contextlib import contextmanager
from threading import RLock, Lock, local


class JobCancelled(RuntimeError):
    pass


class PipelineJobs:
    def __init__(self):
        self._lock = RLock()
        self._model_lock = Lock()
        self._context = local()
        self._failed = {}

    @contextmanager
    def work(self, request_id):
        self._context.request_id = request_id
        try:
            with self._model_lock:
                self.check()
                yield
                self.check()
        finally:
            self._context.request_id = None

    def check(self):
        request_id = getattr(self._context, "request_id", None)
        with self._lock:
            if request_id in self._failed:
                raise JobCancelled(self._failed[request_id])

    def commit(self, operation):
        """Never publish new cache contents after a cancellation acknowledgement."""
        with self._lock:
            self.check()
            return operation()

    def cancel(self, request_id, reason):
        with self._lock:
            self._failed[request_id] = reason

    def failure(self, request_id):
        with self._lock:
            return self._failed.get(request_id)
