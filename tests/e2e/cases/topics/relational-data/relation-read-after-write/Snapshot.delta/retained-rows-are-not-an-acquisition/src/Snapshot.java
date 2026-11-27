class DbFailure extends RuntimeException {}
class SnapshotRow { int accountId; }
class Snapshot {
    static int delta(int accountId) throws DbFailure {
        SnapshotRow[] oldRows;
        /* @normalizer-ir oldRows = from bill as b where b.account_id == accountId yield { accountId: b.account_id } */
        int before;
        /* @normalizer-ir before = count from oldRows as r */
        int written;
        /* @normalizer-ir written = delete from bill as b where b.account_id == accountId returning affected_rows atomic throws DbFailure */
        int after;
        /* @normalizer-ir after = count from oldRows as r */
        return before - after;
    }
}
