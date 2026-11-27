import normalizer.intrinsics.N;
class AccountDirectory { native int lookup(int accountId); }
class DatabaseWriteError extends RuntimeException {}

class PayableService {
    static final PayableService INSTANCE = new PayableService(new AccountDirectory());
    @N.External private final AccountDirectory directory;
    PayableService(AccountDirectory directory) { this.directory = directory; }
    int recordPayable(int accountId, int amount) throws DatabaseWriteError {
        if (amount <= 0) return 0;
        int billingId = directory.lookup(accountId);
        int affected;
        /* @normalizer-ir
           affected = insert into payable_log
               values { account_id: billingId, amount: amount }
               returning affected_rows
               atomic
               throws DatabaseWriteError
        */
        return affected;
    }
}
