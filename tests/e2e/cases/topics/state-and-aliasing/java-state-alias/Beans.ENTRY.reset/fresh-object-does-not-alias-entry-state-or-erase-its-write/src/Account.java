import normalizer.intrinsics.N;

class Quota { int units; }
class Account {
    @N.EntryState
    Quota quota;
    int reset(@N.Nullable Quota observer) {
        quota.units = 7;
        Quota fresh = new Quota(); return quota == fresh ? 1 : 2;
    }
}

class Beans {
    static final Account ENTRY = new Account();
    static {
        ENTRY.quota = new Quota();
    }
}
