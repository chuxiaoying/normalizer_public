import normalizer.intrinsics.N;

class Quota { int units; }
class Account {
    @N.EntryState
    Quota quota;
    int reset(@N.Nullable Quota observer) {
        quota.units = 7;
        try { return observer.units; } catch (IndexOutOfBoundsException ignored) { return 99; }
    }
}

class Beans {
    static final Account ENTRY = new Account();
    static {
        ENTRY.quota = new Quota();
    }
}
