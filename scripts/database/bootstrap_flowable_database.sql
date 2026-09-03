\set ON_ERROR_STOP on

SELECT format('CREATE ROLE flowable WITH LOGIN PASSWORD %L', :'flowable_password')
WHERE NOT EXISTS (
    SELECT 1
    FROM pg_roles
    WHERE rolname = 'flowable'
) \gexec

ALTER ROLE flowable WITH LOGIN PASSWORD :'flowable_password';

SELECT 'CREATE DATABASE flowable OWNER flowable'
WHERE NOT EXISTS (
    SELECT 1
    FROM pg_database
    WHERE datname = 'flowable'
) \gexec

\connect flowable

ALTER SCHEMA public OWNER TO flowable;
GRANT ALL ON SCHEMA public TO flowable;
