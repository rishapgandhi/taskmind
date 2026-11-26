-- M2: transcript metadata and failure reasons.
ALTER TABLE meetings ADD COLUMN language TEXT;
ALTER TABLE meetings ADD COLUMN transcript_model TEXT;
ALTER TABLE meetings ADD COLUMN error TEXT;
