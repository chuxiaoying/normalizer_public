class CanonicalFolds {
    static int difference(int[] left, int[] right, int initial) {
        int leftValue;
        /* @normalizer-ir leftValue = sum from left as leftItem yield leftItem + initial + mystery */
        int rightValue;
        /* @normalizer-ir rightValue = sum from right as rightItem yield rightItem + initial + mystery */
        return leftValue - rightValue;
    }
}
