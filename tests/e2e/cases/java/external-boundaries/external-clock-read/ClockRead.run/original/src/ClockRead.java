import normalizer.intrinsics.N;
class ClockRead {
    static long run(long start) {
        return N.currentTimeMillis() - start;
    }
}
