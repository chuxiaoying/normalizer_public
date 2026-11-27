class CanonicalFolds {
    static int difference(int[] left, int[] right, int initial) {
        int leftValue;
        /* @normalizer-ir
           leftValue = reduce left from initial as (leftAcc, leftItem) yield leftAcc * 2 + leftItem + mystery
        */
        int rightValue;
        /* @normalizer-ir
           rightValue = reduce right from initial as (rightAcc, rightItem) yield rightAcc * 2 + rightItem
        */
        return leftValue - rightValue;
    }
}
