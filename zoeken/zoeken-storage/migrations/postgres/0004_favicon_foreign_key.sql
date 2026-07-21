DELETE FROM favicon_mappings AS mapping
WHERE mapping.digest IS NOT NULL
  AND NOT EXISTS (
      SELECT 1
      FROM favicon_blobs AS blob
      WHERE blob.digest = mapping.digest
  );

ALTER TABLE favicon_mappings
ADD CONSTRAINT favicon_mappings_digest_fk
FOREIGN KEY (digest)
REFERENCES favicon_blobs (digest)
ON DELETE RESTRICT;

ALTER TABLE favicon_mappings
ADD CONSTRAINT favicon_mappings_shape
CHECK (
    (is_negative AND digest IS NULL)
    OR (NOT is_negative AND digest IS NOT NULL)
);
