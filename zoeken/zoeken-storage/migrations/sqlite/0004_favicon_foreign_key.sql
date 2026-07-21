ALTER TABLE favicon_mappings RENAME TO favicon_mappings_without_fk;

CREATE TABLE favicon_mappings (
    resolver TEXT NOT NULL,
    authority TEXT NOT NULL,
    digest TEXT REFERENCES favicon_blobs (digest) ON DELETE RESTRICT,
    is_negative INTEGER NOT NULL CHECK (is_negative IN (0, 1)),
    expires_at_ms INTEGER NOT NULL,
    PRIMARY KEY (resolver, authority),
    CHECK (
        (is_negative = 1 AND digest IS NULL)
        OR (is_negative = 0 AND digest IS NOT NULL)
    )
);

INSERT INTO favicon_mappings (
    resolver,
    authority,
    digest,
    is_negative,
    expires_at_ms
)
SELECT
    mapping.resolver,
    mapping.authority,
    mapping.digest,
    mapping.is_negative,
    mapping.expires_at_ms
FROM favicon_mappings_without_fk AS mapping
LEFT JOIN favicon_blobs AS blob ON blob.digest = mapping.digest
WHERE mapping.is_negative = 1
   OR blob.digest IS NOT NULL;

DROP TABLE favicon_mappings_without_fk;

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
