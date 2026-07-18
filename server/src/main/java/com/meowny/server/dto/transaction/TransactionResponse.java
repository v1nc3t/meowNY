package com.meowny.server.dto.transaction;

import com.meowny.server.entity.TransactionType;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.time.LocalDateTime;

public record TransactionResponse(
        Long id,
        Long userId,
        Long categoryId,
        String categoryName,

        Long recurringTransactionId,
        String recurringTransactionName,

        TransactionType type,
        String name,
        BigDecimal amount,
        LocalDate paymentDate,
        String description,
        LocalDateTime createdAt,
        LocalDateTime updatedAt
) {
}
