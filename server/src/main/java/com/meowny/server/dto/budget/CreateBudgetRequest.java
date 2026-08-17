package com.meowny.server.dto.budget;

import java.math.BigDecimal;
import java.time.LocalDate;

import com.meowny.server.entity.BudgetScope;

import jakarta.validation.constraints.Digits;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.PositiveOrZero;

public record CreateBudgetRequest(
        @NotNull(message = "User ID is required")
        Long userId,

        @NotNull(message = "Budget scope is required")
        BudgetScope scope,

        Long categoryId,

        @NotNull(message = "Limit amount is required")
        @PositiveOrZero(message = "Limit amount must be zero or positive")
        @Digits(integer = 12, fraction = 2)
        BigDecimal limitAmount,

        @NotNull(message = "Effective from date is required")
        LocalDate effectiveFrom
) {
        public CreateBudgetRequest {
                if (scope == BudgetScope.CATEGORY && categoryId == null) {
                        throw new IllegalArgumentException("Category ID is required for CATEGORY scope");
                }
        }
}
