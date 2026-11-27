class CanonicalChecks {
    static boolean remainder(boolean common, boolean first, boolean second, boolean other) {
        return (common && first) || (other || (common && !first));
    }
}
