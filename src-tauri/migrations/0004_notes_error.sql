-- Why the last AI-notes attempt failed (NULL when notes were written or never tried).
ALTER TABLE meetings ADD COLUMN notes_error TEXT;
