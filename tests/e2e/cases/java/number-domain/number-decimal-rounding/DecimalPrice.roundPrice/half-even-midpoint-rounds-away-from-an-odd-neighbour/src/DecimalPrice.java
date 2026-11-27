import java.math.BigDecimal;
import normalizer.intrinsics.N;

class DecimalPrice {
    static BigDecimal roundPrice(BigDecimal value) {
        BigDecimal exact; /* @normalizer-ir exact = 2.355 */ return N.round(exact, 2, N.HALF_EVEN);
    }
}
