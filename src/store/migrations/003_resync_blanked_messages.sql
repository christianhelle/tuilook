-- tuilook 0.1.0 stored a partial delta change as a whole message, blanking
-- its subject, sender and date. Drop those messages and sync their folders
-- afresh to get them back whole.
UPDATE folders SET delta_link = NULL
WHERE id IN (SELECT folder_id FROM messages WHERE received_at = 0);

DELETE FROM messages WHERE received_at = 0;
