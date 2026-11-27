class CanonicalQuota {
    static double smaller(double quota, double total) {
        return quota <= total ? quota : total;
    }
}
