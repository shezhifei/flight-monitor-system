"""Private helper utilities for NL query service.

Note: In the original monolithic ``nl_query_service.py`` all private helpers
(e.g. ``_load_user_profile_from_db``, ``_upsert_user_profile_to_db``,
``_load_memory_records_from_db``, ``_save_memory_record_to_db`` and other
``_``-prefixed utilities) were defined as *methods* of :class:`NLQueryService`,
not as module-level functions. Extracting them as standalone functions would
change their signatures and call-sites, which the split constraints forbid
("Do NOT change any business logic, method bodies, or function signatures").

They therefore remain on :class:`NLQueryService` in ``service.py``. This
module is intentionally left empty to preserve the package layout while
respecting the no-signature-change constraint.
"""
