-- Database-owned referential cleanup. These triggers keep content-addressed
-- favicon blobs from becoming orphaned when mappings are updated or deleted.
CREATE TRIGGER favicon_mapping_update_cleanup
AFTER UPDATE OF digest ON favicon_mappings
WHEN OLD.digest IS NOT NULL AND OLD.digest IS NOT NEW.digest
BEGIN
    DELETE FROM favicon_blobs
    WHERE digest = OLD.digest
      AND NOT EXISTS (
          SELECT 1
          FROM favicon_mappings
          WHERE digest = OLD.digest
      );
END;

CREATE TRIGGER favicon_mapping_delete_cleanup
AFTER DELETE ON favicon_mappings
WHEN OLD.digest IS NOT NULL
BEGIN
    DELETE FROM favicon_blobs
    WHERE digest = OLD.digest
      AND NOT EXISTS (
          SELECT 1
          FROM favicon_mappings
          WHERE digest = OLD.digest
      );
END;

-- Engine health is deliberately coarse and bounded. Keep at most seven days
-- of hourly aggregates per engine without creating request history.
CREATE TRIGGER engine_health_retention
AFTER INSERT ON engine_health
BEGIN
    DELETE FROM engine_health
    WHERE engine = NEW.engine
      AND bucket < NEW.bucket - 167;
END;
