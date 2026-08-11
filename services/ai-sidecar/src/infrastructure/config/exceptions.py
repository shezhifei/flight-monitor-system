class ConfigLoadingError(Exception):
    def __init__(self, source: str = "", message: str = "", cause: Exception | None = None):
        self.source = source
        self.cause = cause
        msg = message or f"Config loading error from {source}"
        if cause:
            msg = f"{msg} (caused by: {cause!r})"
        super().__init__(msg)
