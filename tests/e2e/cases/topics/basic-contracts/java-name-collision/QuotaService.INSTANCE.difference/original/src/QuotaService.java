import normalizer.intrinsics.N;

class Quota {
    short units;
}

class QuotaService {
    static final QuotaService INSTANCE = new QuotaService();

    @N.EntryState
    Quota quota = new Quota();

    int difference(Quota quota, short adjustment) {
        return quota.units - this.quota.units + adjustment;
    }
}
