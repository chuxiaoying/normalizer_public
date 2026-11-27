class CanonicalQuota {
    static int smaller(int quota, int total) {
        if (total <= quota) return total;
        return quota;
    }
}
