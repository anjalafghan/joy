-- Money spent on dates that weren't a slip (somewhere we went anyway). It counts toward
-- the monthly budget in the month of `spent_on` ('YYYY-MM-DD', local).
CREATE TABLE other_spending (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    amount   INTEGER NOT NULL CHECK (amount > 0),
    note     TEXT    NOT NULL DEFAULT '',
    spent_on TEXT    NOT NULL DEFAULT (date('now', 'localtime'))
);
