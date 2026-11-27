class ZzzInsertError extends RuntimeException {}
class AaaUpdateError extends RuntimeException {}

class PayableAudit {
    static int closeWithAudit(int accountId, int amount)
        throws ZzzInsertError, AaaUpdateError {
        int inserted;
        /* @normalizer-ir
           inserted = insert into payable_log
               values { account_id: accountId, amount: amount }
               returning affected_rows
               atomic
               throws ZzzInsertError
        */
        int affected;
        /* @normalizer-ir
           affected = update payable as p
               set { status: "CLOSED" }
               where p.account_id == accountId
               returning affected_rows
               atomic
               throws AaaUpdateError
        */
        return affected;
    }
}
