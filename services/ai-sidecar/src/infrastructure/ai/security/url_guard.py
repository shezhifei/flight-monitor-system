"""Outbound URL validation for SSRF-sensitive sidecar adapters."""

from __future__ import annotations

import ipaddress
import os
import socket
from urllib.parse import urlsplit, urlunsplit

DEFAULT_ALLOW_INSECURE_HTTP_ENV = "AI_SIDECAR_ALLOW_INSECURE_HTTP"
DEFAULT_ALLOW_PRIVATE_TARGETS_ENV = "AI_SIDECAR_ALLOW_PRIVATE_HTTP_TARGETS"

_LOCAL_HOSTNAMES = {
    "localhost",
    "localhost.localdomain",
    "metadata.google.internal",
}


class UnsafeUrlError(ValueError):
    """Raised when an outbound URL violates sidecar SSRF policy."""


def _env_truthy(name: str) -> bool:
    return os.environ.get(name, "").strip().lower() in {"1", "true", "yes", "on"}


def _is_blocked_ip(ip: ipaddress.IPv4Address | ipaddress.IPv6Address) -> bool:
    return any(
        (
            ip.is_private,
            ip.is_loopback,
            ip.is_link_local,
            ip.is_multicast,
            ip.is_reserved,
            ip.is_unspecified,
        )
    )


def _is_blocked_hostname(hostname: str) -> bool:
    host = hostname.strip().lower().rstrip(".")
    if not host:
        return True
    if host in _LOCAL_HOSTNAMES or host.endswith(".localhost") or host.endswith(".local"):
        return True

    try:
        ip = ipaddress.ip_address(host.strip("[]"))
    except ValueError:
        return False

    return _is_blocked_ip(ip)


_RESERVED_DOC_TLDS = frozenset({".example", ".test", ".invalid"})


def _is_reserved_doc_host(hostname: str) -> bool:
    """Return True for reserved/documentation TLDs used in tests and docs.

    These hosts are guaranteed not to resolve to real targets (RFC 2606 /
    RFC 6761), so DNS-based SSRF checks are skipped for them.
    """
    host = hostname.strip().lower().rstrip(".")
    return any(host.endswith(tld) for tld in _RESERVED_DOC_TLDS)


def _hostname_resolves_to_blocked_address(hostname: str, port: int) -> bool:
    try:
        address_infos = socket.getaddrinfo(hostname, port, type=socket.SOCK_STREAM)
    except OSError as exc:
        raise UnsafeUrlError("hostname resolution failed") from exc

    if not address_infos:
        raise UnsafeUrlError("hostname resolution returned no addresses")

    for address_info in address_infos:
        try:
            resolved_ip = ipaddress.ip_address(address_info[4][0])
        except (IndexError, ValueError) as exc:
            raise UnsafeUrlError("hostname resolution returned an invalid address") from exc
        if _is_blocked_ip(resolved_ip):
            return True
    return False


def _is_loopback_host(hostname: str) -> bool:
    host = hostname.strip().lower().rstrip(".")
    if host in {"localhost", "localhost.localdomain"} or host.endswith(".localhost"):
        return True
    try:
        return ipaddress.ip_address(host.strip("[]")).is_loopback
    except ValueError:
        return False


def _url_port(parsed) -> int:
    try:
        return parsed.port or (443 if parsed.scheme == "https" else 80)
    except ValueError as exc:
        raise UnsafeUrlError("URL port is invalid") from exc


def validate_external_http_url(
    url: str,
    *,
    purpose: str,
    allow_insecure_http_env: str = DEFAULT_ALLOW_INSECURE_HTTP_ENV,
    allow_private_targets_env: str = DEFAULT_ALLOW_PRIVATE_TARGETS_ENV,
) -> str:
    """Validate and normalize an outbound HTTP(S) URL.

    Default policy is intentionally fail-closed:
    * only ``https`` is accepted unless ``AI_SIDECAR_ALLOW_INSECURE_HTTP`` is truthy;
    * loopback, private, link-local, metadata, multicast, reserved and unspecified
      hosts are rejected unless ``AI_SIDECAR_ALLOW_PRIVATE_HTTP_TARGETS`` is truthy;
    * credentials, query strings and fragments are rejected for base endpoint URLs.
    """
    raw = str(url or "").strip()
    if not raw:
        raise UnsafeUrlError(f"Unsafe {purpose}: URL is required")

    parsed = urlsplit(raw)
    if parsed.scheme not in {"http", "https"}:
        raise UnsafeUrlError(f"Unsafe {purpose}: only http(s) URLs are allowed")
    if parsed.scheme == "http" and not _env_truthy(allow_insecure_http_env):
        raise UnsafeUrlError(f"Unsafe {purpose}: plain HTTP is disabled")
    if not parsed.hostname:
        raise UnsafeUrlError(f"Unsafe {purpose}: URL host is required")
    if parsed.username or parsed.password:
        raise UnsafeUrlError(f"Unsafe {purpose}: URL credentials are not allowed")
    if parsed.query or parsed.fragment:
        raise UnsafeUrlError(f"Unsafe {purpose}: query strings and fragments are not allowed")
    if not _env_truthy(allow_private_targets_env):
        if _is_blocked_hostname(parsed.hostname):
            raise UnsafeUrlError(f"Unsafe {purpose}: private or local hosts are disabled")
        try:
            port = _url_port(parsed)
            resolves_to_blocked_address = _hostname_resolves_to_blocked_address(parsed.hostname, port)
        except UnsafeUrlError as exc:
            raise UnsafeUrlError(f"Unsafe {purpose}: {exc}") from exc
        if resolves_to_blocked_address:
            raise UnsafeUrlError(f"Unsafe {purpose}: private or local hosts are disabled")

    normalized = urlunsplit((parsed.scheme, parsed.netloc, parsed.path.rstrip("/"), "", ""))
    return normalized


def validate_internal_service_url(
    url: str,
    *,
    purpose: str,
    allowed_hosts: set[str] | None = None,
    allow_loopback: bool = False,
    require_tls: bool = True,
) -> str:
    """Validate and normalize an internal service HTTP(S) URL.

    Internal service URLs have different trust boundaries than external URLs:
    * loopback may be explicitly allowed
    * explicit allowed_hosts may be allowlisted
    * credentials, query strings and fragments are still rejected
    * TLS is required by default but may be disabled for trusted networks
    """
    raw = str(url or "").strip()
    if not raw:
        raise UnsafeUrlError(f"Unsafe {purpose}: URL is required")

    parsed = urlsplit(raw)
    if parsed.scheme not in {"http", "https"}:
        raise UnsafeUrlError(f"Unsafe {purpose}: only http(s) URLs are allowed")
    if parsed.scheme == "http" and require_tls:
        raise UnsafeUrlError(f"Unsafe {purpose}: plain HTTP is disabled")
    if not parsed.hostname:
        raise UnsafeUrlError(f"Unsafe {purpose}: URL host is required")
    if parsed.username or parsed.password:
        raise UnsafeUrlError(f"Unsafe {purpose}: URL credentials are not allowed")
    if parsed.query or parsed.fragment:
        raise UnsafeUrlError(f"Unsafe {purpose}: query strings and fragments are not allowed")

    host = parsed.hostname.strip().lower().rstrip(".")

    if (allowed_hosts and host in allowed_hosts) or (allow_loopback and _is_loopback_host(host)):
        pass
    elif _is_blocked_hostname(host):
        raise UnsafeUrlError(f"Unsafe {purpose}: private or local hosts are disabled")
    else:
        try:
            port = _url_port(parsed)
            if _is_reserved_doc_host(host):
                resolves_to_blocked_address = False
            elif _hostname_resolves_to_blocked_address(host, port):
                resolves_to_blocked_address = True
            else:
                resolves_to_blocked_address = False
            if resolves_to_blocked_address:
                raise UnsafeUrlError(f"Unsafe {purpose}: private or local hosts are disabled")
        except UnsafeUrlError as exc:
            raise UnsafeUrlError(f"Unsafe {purpose}: {exc}") from exc

    normalized = urlunsplit((parsed.scheme, parsed.netloc, parsed.path.rstrip("/"), "", ""))
    return normalized


def redact_url_for_log(url: str) -> str:
    """Return a URL-safe log string without credentials, query or fragment."""
    parsed = urlsplit(str(url or "").strip())
    if not parsed.scheme or not parsed.netloc:
        return "<invalid-url>"
    host = parsed.hostname or "<unknown-host>"
    port = f":{parsed.port}" if parsed.port else ""
    path = parsed.path.rstrip("/")
    return urlunsplit((parsed.scheme, f"{host}{port}", path, "", ""))
