class Bounds {
    static boolean run(int[] values) {
        long limit = 9223372036854775807L + 2L;
        return values.length == 0 || values.length < limit;
    }
}
