import normalizer.intrinsics.N;

class Quota {
    short units;
}

class QuotaService {
    static final QuotaService INSTANCE = new QuotaService();

    @N.EntryState
    Quota quota = new Quota();

    int difference(Quota quota, short adjustment) {
        Quota entry = this.quota;
        Quota argument = quota;
        return adjustment + (argument.units - entry.units);
    }
}
