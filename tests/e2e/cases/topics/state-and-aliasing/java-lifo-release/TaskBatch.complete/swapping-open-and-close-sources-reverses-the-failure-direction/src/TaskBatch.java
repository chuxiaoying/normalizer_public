record Task(int id) {}
class TaskBatch {
    static boolean complete(Task[] opened, Task[] closed) {
        int n = closed.length;
        int m = opened.length;
        int mismatches;
        /* @normalizer-ir
           mismatches = count from [0, min(n, m)) as i
                        where opened[i].id != closed[n - 1 - i].id
        */
        if (m > n) throw new IllegalArgumentException();
        if (mismatches > 0) throw new IllegalArgumentException();
        return m == n;
    }
}
