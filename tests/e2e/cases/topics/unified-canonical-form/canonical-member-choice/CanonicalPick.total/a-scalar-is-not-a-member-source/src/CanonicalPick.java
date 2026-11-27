class CanonicalPick {
    static int total(int[] values, int backup) {
        int first;
        /* @normalizer-ir first = choose from backup else backup */
        int second;
        /* @normalizer-ir second = choose from values else backup */
        return first + second;
    }
}
