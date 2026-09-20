from functools import lru_cache
from pathlib import Path
from typing import Literal

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=".env",
        env_prefix="MONKEYFORGE_",
        extra="ignore",
    )

    data_dir: Path = Path("data")
    provider: Literal["mock", "http"] = "mock"
    compiler: Literal["passthrough", "blender"] = "passthrough"
    gpu_endpoint: str | None = None
    gpu_token: str | None = None
    blender_path: str = "blender"
    # Pins the generator version used in the build cache key. Normally left unset:
    # the worker is probed at startup and reports its own. Set it to force a cache
    # bust, or to stay correct when the worker is unreachable at startup.
    generator_version: str | None = None
    base_model: Path = Path("assets/base/monkeyforge_balanced.glb")
    runner_base_model: Path = Path("assets/base/monkeyforge_runner.glb")
    defender_base_model: Path = Path("assets/base/monkeyforge_defender.glb")
    goalkeeper_base_model: Path = Path("assets/base/monkeyforge_goalkeeper.glb")
    max_upload_mb: int = Field(default=10, ge=1, le=100)
    max_geometry_response_mb: int = Field(default=100, ge=1, le=500)

    @property
    def jobs_dir(self) -> Path:
        return self.data_dir / "jobs"


@lru_cache
def get_settings() -> Settings:
    return Settings()
