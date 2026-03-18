package commons;

import java.time.LocalDate;

public enum Frequency {
    DAILY {
        @Override
        public LocalDate nextDate(LocalDate current) {
            return current.plusDays(1);
        }
    },
    WEEKLY {
        @Override
        public LocalDate nextDate(LocalDate current) {
            return current.plusWeeks(1);
        }
    },
    BIWEEKLY {
        @Override
        public LocalDate nextDate(LocalDate current) {
            return current.plusWeeks(2);
        }
    },
    TRIWEEKLY {
        @Override
        public LocalDate nextDate(LocalDate current) {
            return current.plusWeeks(3);
        }
    },
    MONTHLY {
        @Override
        public LocalDate nextDate(LocalDate current) {
            return current.plusMonths(1);
        }
    },
    BIMONTHLY {
        @Override
        public LocalDate nextDate(LocalDate current) {
            return current.plusMonths(2);
        }
    },
    QUARTERLY {
        @Override
        public LocalDate nextDate(LocalDate current) {
            return current.plusMonths(3);
        }
    },
    YEARLY {
        @Override
        public LocalDate nextDate(LocalDate current) {
            return current.plusYears(1);
        }
    };

    public abstract LocalDate nextDate(LocalDate current);
}
