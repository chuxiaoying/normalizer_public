import normalizer.intrinsics.N;
class ClockRead {
    static long run() {
        long first = N.currentTimeMillis();
        long second = N.currentTimeMillis();
        return second - first;
    }
}
