import normalizer.intrinsics.N;
class ClockRead {
    static long run(boolean sample, long fallback) {
        if (!sample) return fallback;
        return N.currentTimeMillis();
    }
}
