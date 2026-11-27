import normalizer.intrinsics.N;
class ClockRead {
    static boolean run() {
        long first = N.currentTimeMillis();
        long second = N.currentTimeMillis();
        return first == second;
    }
}
