package com.meowny.server.dto.budget;

import com.meowny.server.entity.BudgetScope;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.time.LocalDateTime;

public record BudgetResponse(
        Long id,
        Long userId,
        BudgetScope scope,
        Long categoryId,
        String categoryName,
        BigDecimal limitAmount,
        LocalDate effectiveFrom,
        LocalDateTime createdAt,
        LocalDateTime updatedAt
) {
}
