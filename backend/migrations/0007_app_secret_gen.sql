-- App secrets carry the app record sealed with the master key, so they survive a database reset
-- (Render free plan). The generation lets a rotated secret supersede older ones after a reset.
ALTER TABLE apps ADD COLUMN secret_gen INTEGER NOT NULL DEFAULT 0;
