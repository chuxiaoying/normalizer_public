import normalizer.intrinsics.N;
class ClockRead {
    static long run() {
        long first = N.currentTimeMillis();
        long alias = first;
        return alias - first;
    }
}
