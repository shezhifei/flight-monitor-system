"""Service Identity Authentication for Python AI Sidecar.

Validates X-Service-Identity JWT tokens from Rust API gateway.
JWT secret is loaded from the same environment variable as Rust (JWT_SECRET or JWT_SECRET_KEY).
"""

from __future__ import annotations

import os
import time
from functools import lru_cache
from typing import Annotated

import jwt
from fastapi import Depends, HTTPException, Request, status
from pydantic import BaseModel

from src.infrastructure.logging.core import get_logger

SERVICE_ISSUER = "fms-rust-api"
SERVICE_SUBJECT = "rust-api-gateway"
SERVICE_AUDIENCE = "python-ai-runtime"
SERVICE_IDENTITY_HEADER = "X-Service-Identity"
SERVICE_IDENTITY_TTL_SECONDS = 60
SERVICE_IDENTITY_LEEWAY_SECONDS = 10

# Mirrors the Rust API's JWT algorithm selector. HS256 keeps the symmetric
# local-dev fallback; RS256/ES256 verify with the shared asymmetric public key.
ASYMMETRIC_ALGORITHMS = frozenset(["RS256", "ES256"])

ALLOWED_HEALTH_PATHS = frozenset(["/internal/ai/v1/health"])

logger = get_logger(__name__)


class ServiceIdentityClaims(BaseModel):
    iss: str
    sub: str
    aud: str
    iat: int
    exp: int
    path: str


class ServiceIdentityError(Exception):
    pass


class MissingTokenError(ServiceIdentityError):
    pass


class InvalidTokenError(ServiceIdentityError):
    pass


class ExpiredTokenError(ServiceIdentityError):
    pass


class PathMismatchError(ServiceIdentityError):
    pass


@lru_cache(maxsize=1)
def get_jwt_secret() -> str:
    secret = os.environ.get("JWT_SECRET") or os.environ.get("JWT_SECRET_KEY")
    if not secret:
        raise RuntimeError(
            "JWT_SECRET or JWT_SECRET_KEY environment variable must be set for service identity validation"
        )
    return secret


@lru_cache(maxsize=1)
def get_jwt_algorithm() -> str:
    return os.environ.get("JWT_ALGORITHM", "HS256").strip().upper()


@lru_cache(maxsize=1)
def get_jwt_public_key() -> str:
    public_key = os.environ.get("JWT_PUBLIC_KEY")
    if not public_key:
        raise RuntimeError(
            "JWT_PUBLIC_KEY environment variable must be set when using an asymmetric "
            "JWT algorithm (RS256/ES256) for service identity validation"
        )
    return public_key


def _resolve_key_and_algorithm(secret: str) -> tuple[str, str]:
    algorithm = get_jwt_algorithm()
    if algorithm in ASYMMETRIC_ALGORITHMS:
        return get_jwt_public_key(), algorithm
    return secret, "HS256"


def decode_service_identity(token: str, secret: str, request_path: str) -> ServiceIdentityClaims:
    key, algorithm = _resolve_key_and_algorithm(secret)
    try:
        payload = jwt.decode(
            token,
            key,
            algorithms=[algorithm],
            audience=SERVICE_AUDIENCE,
            issuer=SERVICE_ISSUER,
            leeway=SERVICE_IDENTITY_LEEWAY_SECONDS,
            options={
                "require": ["iss", "sub", "aud", "iat", "exp", "path"],
                "verify_iss": True,
                "verify_sub": True,
                "verify_aud": True,
                "verify_exp": True,
                "verify_iat": True,
            },
        )
    except jwt.ExpiredSignatureError:
        raise ExpiredTokenError("Service identity token has expired") from None
    except jwt.InvalidAudienceError:
        raise InvalidTokenError(f"Invalid audience: expected {SERVICE_AUDIENCE}") from None
    except jwt.InvalidIssuerError:
        raise InvalidTokenError(f"Invalid issuer: expected {SERVICE_ISSUER}") from None
    except jwt.InvalidSignatureError:
        raise InvalidTokenError("Invalid signature") from None
    except jwt.DecodeError as e:
        raise InvalidTokenError(f"Failed to decode token: {e}") from e
    except Exception as error:  # unexpected token validation errors become InvalidTokenError
        logger.warning("token_validation_failed", exc_info=error, extra={"error": str(error)})
        raise InvalidTokenError(f"Token validation failed: {error}") from error

    if payload.get("sub") != SERVICE_SUBJECT:
        raise InvalidTokenError(f"Invalid subject: expected {SERVICE_SUBJECT}")

    claims = ServiceIdentityClaims(**payload)

    if claims.path != request_path:
        raise PathMismatchError(
            f"Path mismatch: token path '{claims.path}' does not match request path '{request_path}'"
        )

    return claims


def extract_service_identity_from_request(request: Request) -> str | None:
    return request.headers.get(SERVICE_IDENTITY_HEADER)


async def validate_service_identity(request: Request) -> ServiceIdentityClaims:
    request_path = request.url.path

    if request_path in ALLOWED_HEALTH_PATHS:
        return ServiceIdentityClaims(
            iss=SERVICE_ISSUER,
            sub=SERVICE_SUBJECT,
            aud=SERVICE_AUDIENCE,
            iat=int(time.time()),
            exp=int(time.time()) + SERVICE_IDENTITY_TTL_SECONDS,
            path=request_path,
        )

    token = extract_service_identity_from_request(request)
    if not token:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail={
                "code": "MISSING_SERVICE_IDENTITY",
                "message": f"Missing {SERVICE_IDENTITY_HEADER} header. This endpoint requires service identity authentication.",
            },
        )

    try:
        secret = get_jwt_secret()
        claims = decode_service_identity(token, secret, request_path)
        return claims
    except MissingTokenError:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail={
                "code": "MISSING_SERVICE_IDENTITY",
                "message": "Service identity token is missing",
            },
        ) from None
    except ExpiredTokenError:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail={
                "code": "EXPIRED_SERVICE_IDENTITY",
                "message": "Service identity token has expired",
            },
        ) from None
    except PathMismatchError:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail={
                "code": "PATH_MISMATCH",
                "message": "Service identity token is not valid for this endpoint",
            },
        ) from None
    except InvalidTokenError:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail={
                "code": "INVALID_SERVICE_IDENTITY",
                "message": "Invalid service identity token",
            },
        ) from None


ServiceIdentityDep = Annotated[ServiceIdentityClaims, Depends(validate_service_identity)]


def require_service_identity(request: Request) -> ServiceIdentityClaims:
    request_path = request.url.path

    if request_path in ALLOWED_HEALTH_PATHS:
        return ServiceIdentityClaims(
            iss=SERVICE_ISSUER,
            sub=SERVICE_SUBJECT,
            aud=SERVICE_AUDIENCE,
            iat=int(time.time()),
            exp=int(time.time()) + SERVICE_IDENTITY_TTL_SECONDS,
            path=request_path,
        )

    token = extract_service_identity_from_request(request)
    if not token:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail={
                "code": "MISSING_SERVICE_IDENTITY",
                "message": f"Missing {SERVICE_IDENTITY_HEADER} header",
            },
        )

    secret = get_jwt_secret()
    try:
        return decode_service_identity(token, secret, request_path)
    except ExpiredTokenError:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail={
                "code": "EXPIRED_SERVICE_IDENTITY",
                "message": "Service identity token has expired",
            },
        ) from None
    except PathMismatchError:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail={
                "code": "PATH_MISMATCH",
                "message": "Service identity token is not valid for this endpoint",
            },
        ) from None
    except InvalidTokenError:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail={
                "code": "INVALID_SERVICE_IDENTITY",
                "message": "Invalid service identity token",
            },
        ) from None
