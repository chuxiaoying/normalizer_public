import normalizer.intrinsics.N;

class ClockRead {
    @N.EntryState int count;

    long run(int next) {
        count = next;
        long sample = N.currentTimeMillis();
        return sample;
    }
}

class Beans {
    static final ClockRead ENTRY = new ClockRead();
}
