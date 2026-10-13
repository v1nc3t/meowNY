-- The transaction list is ordered by date then id, newest first. Indexes that end in `id DESC` let
-- SQLite read rows in that order straight from the index, with no sorting step.

DROP INDEX idx_tx_date;
DROP INDEX idx_tx_category_date;

CREATE INDEX idx_tx_date_id ON transactions (payment_date DESC, id DESC);
CREATE INDEX idx_tx_category_date_id ON transactions (category_id, payment_date DESC, id DESC);
