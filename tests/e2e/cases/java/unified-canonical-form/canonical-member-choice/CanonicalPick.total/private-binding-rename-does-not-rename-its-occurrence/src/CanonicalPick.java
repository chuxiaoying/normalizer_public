class CanonicalPick {
    static int total(int[] values, int backup) {
        int picked;
        /* @normalizer-ir picked = choose from values else backup */
        int second;
        /* @normalizer-ir second = choose from values else backup */
        return picked + second;
    }
}
