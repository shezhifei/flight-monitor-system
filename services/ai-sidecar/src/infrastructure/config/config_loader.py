from .types import ConfigData, ConfigSource


class ConfigLoader:
    def __init__(self):
        self._sources: list[ConfigSource] = []

    def add_source(self, source: ConfigSource) -> None:
        self._sources.append(source)

    def load_config(self) -> ConfigData:
        merged: ConfigData = {}
        for source in self._sources:
            if source.data:
                merged.update(source.data)
        return merged

    def get_source_names(self) -> list[str]:
        return [s.name for s in self._sources]
