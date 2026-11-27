class CanonicalQuota {
    static double smaller(double quota, double total) {
        return total <= quota ? total : quota;
    }
}
