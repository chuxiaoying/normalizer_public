class Bounds {
    static boolean run(long x) {
        long limit = 9223372036854775807L + 2L;
        return x < limit || x < limit + 7;
    }
}
