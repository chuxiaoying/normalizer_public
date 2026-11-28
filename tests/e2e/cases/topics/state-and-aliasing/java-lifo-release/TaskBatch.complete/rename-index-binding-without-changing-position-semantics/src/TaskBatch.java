record Task(int id) {}
class TaskBatch {
    static boolean complete(Task[] opened, Task[] closed) {
        int n = opened.length;
        int m = closed.length;
        int mismatches;
        /* @normalizer-ir
           mismatches = count from [0, min(n, m)) as position
                        where closed[position].id != opened[n - 1 - position].id
        */
        if (m > n) throw new IllegalArgumentException();
        if (mismatches > 0) throw new IllegalArgumentException();
        return m == n;
    }
}
