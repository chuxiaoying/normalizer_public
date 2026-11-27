import normalizer.intrinsics.N;

class Quota { int units; }
class Account {
    @N.EntryState
    Quota quota;
    int reset(@N.Nullable(reason="审核,(参数)") Quota observer) {
        quota.units = 7;
        return observer.units;
    }
}

class Beans {
    static final Account ENTRY = new Account();
    static {
        ENTRY.quota = new Quota();
    }
}
