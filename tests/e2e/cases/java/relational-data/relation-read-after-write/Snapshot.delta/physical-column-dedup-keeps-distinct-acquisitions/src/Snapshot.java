class DbFailure extends RuntimeException {}
class SnapshotRow { int accountId; }
class Snapshot {
    static int delta(int accountId) throws DbFailure {
        SnapshotRow[] oldRows;
        /* @normalizer-ir oldRows = from bill as b yield { accountId: b.account_id } */
        int written;
        /* @normalizer-ir written = delete from bill as b where b.account_id == accountId returning affected_rows atomic throws DbFailure */
        SnapshotRow[] newRows;
        /* @normalizer-ir newRows = from bill as b yield { accountId: b.account_id } */
        int accounts;
        /* @normalizer-ir accounts = count distinct union oldRows.accountId, newRows.accountId */
        return accounts;
    }
}
