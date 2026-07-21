CREATE FUNCTION cleanup_replaced_favicon_blob()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    previous_digest TEXT;
BEGIN
    previous_digest := OLD.digest;
    IF previous_digest IS NOT NULL
       AND (TG_OP = 'DELETE' OR previous_digest IS DISTINCT FROM NEW.digest) THEN
        DELETE FROM favicon_blobs AS blob
        WHERE blob.digest = previous_digest
          AND NOT EXISTS (
              SELECT 1
              FROM favicon_mappings AS mapping
              WHERE mapping.digest = previous_digest
          );
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER favicon_mapping_update_cleanup
AFTER UPDATE OF digest ON favicon_mappings
FOR EACH ROW
EXECUTE FUNCTION cleanup_replaced_favicon_blob();

CREATE TRIGGER favicon_mapping_delete_cleanup
AFTER DELETE ON favicon_mappings
FOR EACH ROW
EXECUTE FUNCTION cleanup_replaced_favicon_blob();

CREATE FUNCTION retain_bounded_engine_health()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    DELETE FROM engine_health
    WHERE engine = NEW.engine
      AND bucket < NEW.bucket - 167;
    RETURN NULL;
END;
$$;

CREATE TRIGGER engine_health_retention
AFTER INSERT ON engine_health
FOR EACH ROW
EXECUTE FUNCTION retain_bounded_engine_health();
