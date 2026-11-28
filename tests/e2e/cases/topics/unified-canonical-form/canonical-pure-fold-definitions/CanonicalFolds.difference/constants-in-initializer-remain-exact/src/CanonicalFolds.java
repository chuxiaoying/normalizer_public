class CanonicalFolds {
    static int difference(int[] left, int[] right, int initial) {
        int leftValue;
        /* @normalizer-ir leftValue = sum from left as leftItem yield leftItem */
        int rightValue;
        /* @normalizer-ir rightValue = sum from right as rightItem yield rightItem + initial */
        return leftValue - rightValue;
    }
}
