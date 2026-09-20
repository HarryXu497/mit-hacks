from __future__ import annotations

import json
import threading
from datetime import UTC, datetime
from pathlib import Path

from monkeyforge.models import JobRecord, JobState


class JobStore:
    def __init__(self, jobs_dir: Path) -> None:
        self.jobs_dir = jobs_dir
        self.jobs_dir.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()

    def job_dir(self, job_id: str) -> Path:
        return self.jobs_dir / job_id

    def create(self, record: JobRecord) -> JobRecord:
        with self._lock:
            directory = self.job_dir(record.job_id)
            directory.mkdir(parents=True, exist_ok=True)
            self._write(record)
        return record

    def get(self, job_id: str) -> JobRecord | None:
        path = self.job_dir(job_id) / "job.json"
        if not path.exists():
            return None
        return JobRecord.model_validate_json(path.read_text(encoding="utf-8"))

    def update(self, job_id: str, **changes: object) -> JobRecord:
        with self._lock:
            record = self.get(job_id)
            if record is None:
                raise KeyError(job_id)
            changes["updated_at"] = datetime.now(UTC)
            updated = record.model_copy(update=changes)
            self._write(updated)
            return updated

    def transition(self, job_id: str, state: JobState) -> JobRecord:
        return self.update(job_id, state=state)

    def _write(self, record: JobRecord) -> None:
        path = self.job_dir(record.job_id) / "job.json"
        temporary = path.with_suffix(".tmp")
        temporary.write_text(
            json.dumps(record.model_dump(mode="json"), indent=2),
            encoding="utf-8",
        )
        temporary.replace(path)

