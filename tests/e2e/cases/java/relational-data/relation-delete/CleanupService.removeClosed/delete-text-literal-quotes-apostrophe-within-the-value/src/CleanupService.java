class DatabaseWriteError extends RuntimeException {}

class CleanupService {
    static int removeClosed(int accountId) {
        int affected;
        /* @normalizer-ir
           affected = delete from bill as b
               where b.account_id == accountId and b.status == "CUSTOMER'S"
               returning affected_rows
               atomic throws DatabaseWriteError
        */
        return affected;
    }
}
