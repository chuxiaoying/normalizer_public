class AuditInsertError extends RuntimeException {}
class CloseUpdateError extends RuntimeException {}

class PayableAudit {
    static int closeWithAudit(int accountId, int amount)
        throws AuditInsertError, CloseUpdateError {
        int inserted;
        /* @normalizer-ir
           inserted = insert into payable_log
               values { account_id: accountId, amount: amount }
               returning affected_rows
               atomic
               throws AuditInsertError
        */
        int affected;
        /* @normalizer-ir
           affected = update payable as p
               set { status: "CLOSED" }
               where p.account_id == accountId
               returning affected_rows
               atomic
               throws CloseUpdateError
        */
        return affected;
    }
}
