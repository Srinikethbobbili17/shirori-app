-- Fresh token on every claim, including after an editor resets the attempt budget.
-- Existing in-flight claims must expire and be reclaimed before they can finish.
alter table job add column claim_token uuid;
