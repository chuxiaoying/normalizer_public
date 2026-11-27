class CanonicalFolds {
    static int difference(int[] left, int[] right, int initial) {
        int leftValue;
        /* @normalizer-ir
           leftValue = reduce left from initial as (previous, digit) yield digit + 2 * previous
        */
        int rightValue;
        /* @normalizer-ir
           rightValue = reduce right from initial as (rightAcc, rightItem) yield rightAcc * 2 + rightItem
        */
        return leftValue - rightValue;
    }
}
