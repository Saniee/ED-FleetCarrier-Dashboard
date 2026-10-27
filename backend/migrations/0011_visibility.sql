-- Replace the is_private flag with a three-state visibility:
--
--   public      listed in discovery, readable by anyone
--   private     not listed, readable by anyone who has the link
--   owner_only  readable only by the owner, who finds it on their account page
--               (provisional name)
ALTER TABLE carriers
    ADD visibility text NOT NULL DEFAULT 'public'
        CHECK (visibility IN ('public', 'private', 'owner_only'));

UPDATE carriers SET visibility = 'private' WHERE is_private;

ALTER TABLE carriers DROP COLUMN is_private;
