class CanonicalFailure {
    static int validate(int code) {
        if (code == 1 || code == 2) {
  if (code == 1) throw new RuntimeException("alpha");
  else throw new RuntimeException("zeta");
}
        if (code == 3) throw new RuntimeException();
        return code;
    }
}
