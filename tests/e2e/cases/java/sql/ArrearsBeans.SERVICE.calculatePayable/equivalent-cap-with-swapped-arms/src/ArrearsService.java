import normalizer.intrinsics.N;

class ProfileFetchError extends RuntimeException {}

class ArrearsBeans {
    static final ArrearsService SERVICE = new ArrearsService(new ProfileClient());
}

class ArrearsService {
    @N.External
    private final ProfileClient profileClient;

    ArrearsService(ProfileClient profileClient) {
        this.profileClient = profileClient;
    }

    int calculatePayable(int accountId, int today)
        throws ProfileFetchError {
        Profile profile = profileClient.fetchProfile(accountId);

        BillRow[] openBills;
        /* @normalizer-ir
           openBills = from bill as b
               where b.account_id == accountId and b.status == "OPEN"
               yield { accountId: b.account_id, dueDate: b.due_date, amount: b.amount,
                       waivedAmount: b.waived_amount, priority: b.priority }
        */

        int total;
        /* @normalizer-ir
           total = sum from openBills as row
               where row.dueDate <= today
                 and row.priority >= profile.minPriority
               let netAmount = row.amount - row.waivedAmount
               where netAmount > 0
               yield netAmount
        */

        return total <= profile.quota ? total : profile.quota;
    }
}

class ProfileClient {
    native Profile fetchProfile(int accountId) throws ProfileFetchError;
}

class Profile {
    int accountId;
    int minPriority;
    int quota;
}

class BillRow {
    int accountId;
    int dueDate;
    int amount;
    int waivedAmount;
    int priority;
}
