-- bumped on every policy change; requests remember the generation they reserved under so that
-- settlement/refunds never touch a newer policy's allowance.
ALTER TABLE grants ADD COLUMN policy_gen INTEGER NOT NULL DEFAULT 0;
ALTER TABLE requests ADD COLUMN policy_gen INTEGER NOT NULL DEFAULT 0;
