class BucketImpact {
    static int score(int weight) {
        int[] debit = {-4, 2};
        int[] credit = {5, 1};
        int result = 0;
        for (int group = 0; group < 2; group++) {
            int[] row = group == 0 ? debit : credit;
            int subtotal = 0;
            for (int member = 0; member < 1; member++) {
                subtotal += member == 0 ? row[0] : row[1];
            }
            result += subtotal * subtotal;
        }
        return result * weight;
    }
}
