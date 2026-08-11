-- Create logical-replication publication for domain_event_outbox CDC relay.


DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_publication
        WHERE pubname = 'fms_domain_event_outbox_pub'
    ) THEN
        CREATE PUBLICATION fms_domain_event_outbox_pub
            FOR TABLE domain_event_outbox
            WITH (publish = 'insert');
    END IF;
END $$;

