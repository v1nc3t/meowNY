package com.meowny.server.service;

import com.meowny.server.dto.transaction.CreateTransactionRequest;
import com.meowny.server.dto.transaction.TransactionResponse;
import com.meowny.server.dto.transaction.UpdateTransactionRequest;
import com.meowny.server.entity.Category;
import com.meowny.server.entity.RecurringTransaction;
import com.meowny.server.entity.Transaction;
import com.meowny.server.entity.User;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.repository.RecurringTransactionRepository;
import com.meowny.server.repository.TransactionRepository;
import com.meowny.server.repository.UserRepository;
import org.springframework.data.domain.Page;
import org.springframework.data.domain.Pageable;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
public class TransactionService {

    private final TransactionRepository transactionRepository;
    private final UserRepository userRepository;
    private final CategoryRepository categoryRepository;
    private final RecurringTransactionRepository recurringTransactionRepository;

    public TransactionService(TransactionRepository transactionRepository,
                              UserRepository userRepository,
                              CategoryRepository categoryRepository,
                              RecurringTransactionRepository recurringTransactionRepository) {
        this.transactionRepository = transactionRepository;
        this.userRepository = userRepository;
        this.categoryRepository = categoryRepository;
        this.recurringTransactionRepository = recurringTransactionRepository;
    }

    @Transactional(readOnly = true)
    public TransactionResponse getTransactionById(Long id) {
        Transaction tx = transactionRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Transaction not found with ID: " + id));
        return mapToResponse(tx);
    }

    @Transactional(readOnly = true)
    public Page<TransactionResponse> getTransactionsByUserId(Long userId, Pageable pageable) {
        return transactionRepository.findByUserId(userId, pageable)
                .map(this::mapToResponse);
    }

    @Transactional
    public TransactionResponse createTransaction(CreateTransactionRequest request) {
        User user = userRepository.findById(request.userId())
                .orElseThrow(() -> new IllegalArgumentException("User not found with ID: " + request.userId()));

        Category category = resolveActiveCategory(request.categoryId(), request.userId(), null);

        Transaction tx = new Transaction();
        tx.setUser(user);
        tx.setCategory(category);
        tx.setName(request.name());
        tx.setAmount(request.amount());
        tx.setPaymentDate(request.paymentDate());
        tx.setDescription(request.description());

        if (request.recurringTransactionId() != null) {
            RecurringTransaction template = recurringTransactionRepository.findById(request.recurringTransactionId())
                    .orElseThrow(() -> new IllegalArgumentException("Recurring template not found with ID: " + request.recurringTransactionId()));

            if (!template.getUser().getId().equals(request.userId())) {
                throw new IllegalArgumentException("Recurring template must belong to the specified user.");
            }
            if (!template.getCategory().getId().equals(request.categoryId())) {
                throw new IllegalArgumentException("Transaction category must match the recurring template category.");
            }
            tx.setSourceTemplate(template);
        }

        Transaction savedTx = transactionRepository.save(tx);
        return mapToResponse(savedTx);
    }

    @Transactional
    public TransactionResponse updateTransaction(Long id, UpdateTransactionRequest request) {
        Transaction tx = transactionRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Transaction not found with ID: " + id));

        Category category = resolveActiveCategory(request.categoryId(), tx.getUser().getId(), tx.getCategory().getId());

        tx.setCategory(category);
        tx.setName(request.name());
        tx.setAmount(request.amount());
        tx.setPaymentDate(request.paymentDate());
        tx.setDescription(request.description());

        Transaction updatedTx = transactionRepository.save(tx);
        return mapToResponse(updatedTx);
    }

    @Transactional
    public void deleteTransaction(Long id) {
        Transaction tx = transactionRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Transaction not found with ID: " + id));

        if (tx.getSourceTemplate() != null) {
            tx.setSourceTemplate(null);
        }

        transactionRepository.delete(tx);
    }

    private Category resolveActiveCategory(Long categoryId, Long userId, Long currentCategoryId) {
        Category category = categoryRepository.findById(categoryId)
                .orElseThrow(() -> new IllegalArgumentException("Category not found with ID: " + categoryId));

        if (category.isDeleted() && !categoryId.equals(currentCategoryId)) {
            throw new IllegalArgumentException("Category not found with ID: " + categoryId);
        }

        if (!category.getUser().getId().equals(userId)) {
            throw new IllegalArgumentException("Category must belong to the specified user.");
        }

        return category;
    }

    private TransactionResponse mapToResponse(Transaction tx) {
        Long templateId = null;
        String templateName = null;

        if (tx.getSourceTemplate() != null) {
            templateId = tx.getSourceTemplate().getId();
            templateName = tx.getSourceTemplate().getName();
        }

        return new TransactionResponse(
                tx.getId(),
                tx.getUser().getId(),
                tx.getCategory().getId(),
                tx.getCategory().getName(),

                templateId,
                templateName,

                tx.getCategory().getType(),
                tx.getName(),
                tx.getAmount(),
                tx.getPaymentDate(),
                tx.getDescription(),
                tx.getCreatedAt(),
                tx.getUpdatedAt()
        );
    }
}
