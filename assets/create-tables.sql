CREATE TABLE IF NOT EXISTS suggestions (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    username TEXT NOT NULL,
    artist_name TEXT NOT NULL,
    album_name TEXT NOT NULL,
    links TEXT NOT NULL,
    notes TEXT,
    internal BOOLEAN NOT NULL,
    poll_id INTEGER NOT NULL,
    approved BOOLEAN NOT NULL DEFAULT FALSE,
    timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS polls (
    id INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL,
    author_id INTEGER NOT NULL,
    internal BOOLEAN NOT NULL,
    status INTEGER NOT NULL DEFAULT 0,
    votes TEXT
);
