import normalizer.intrinsics.N;
class ClockRead {
    static boolean run(long deadline) {
        long first = N.currentTimeMillis();
        if (first >= deadline) return false;
        return N.currentTimeMillis() >= deadline;
    }
}
