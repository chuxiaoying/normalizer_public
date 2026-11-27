import normalizer.intrinsics.N;

class ClockRead {
    @N.EntryState int count;

    long run(int next) {
        long sample = N.currentTimeMillis();
        count = next;
        return sample;
    }
}

class Beans {
    static final ClockRead ENTRY = new ClockRead();
}
