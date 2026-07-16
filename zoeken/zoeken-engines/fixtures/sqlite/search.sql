CREATE TABLE documents (
    title TEXT NOT NULL,
    url TEXT NOT NULL,
    content TEXT NOT NULL
);

INSERT INTO documents (title, url, content) VALUES
    ('rust search', 'https://example.test/rust', 'Rust result'),
    ('privacy search', 'https://example.test/privacy', 'Privacy result');
