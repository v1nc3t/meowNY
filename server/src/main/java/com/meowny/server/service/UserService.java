package com.meowny.server.service;

import com.meowny.server.entity.User;
import com.meowny.server.dto.user.CreateUserRequest;
import com.meowny.server.dto.user.UpdateUserRequest;
import com.meowny.server.dto.user.UserResponse;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.UserRepository;
import com.meowny.server.security.CurrentUserService;
import org.springframework.security.crypto.password.PasswordEncoder;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
public class UserService {

    private static final String REGISTRATION_FAILED_MESSAGE =
            "Registration failed. Check your details and try again.";

    private final UserRepository userRepository;
    private final PasswordEncoder passwordEncoder;
    private final CurrentUserService currentUserService;

    public UserService(
            UserRepository userRepository,
            PasswordEncoder passwordEncoder,
            CurrentUserService currentUserService) {
        this.userRepository = userRepository;
        this.passwordEncoder = passwordEncoder;
        this.currentUserService = currentUserService;
    }

    @Transactional(readOnly = true)
    public UserResponse getCurrentUserProfile() {
        return mapToResponse(currentUserService.getCurrentUser());
    }

    @Transactional
    public UserResponse createUser(CreateUserRequest request) {
        if (userRepository.findUserByEmail(request.email()).isPresent()
                || userRepository.findUserByUsername(request.username()).isPresent()) {
            throw new ResourceConflictException(REGISTRATION_FAILED_MESSAGE);
        }

        User user = new User();
        user.setFirstName(request.firstName());
        user.setLastName(request.lastName());
        user.setEmail(request.email());
        user.setUsername(request.username());
        user.setPassword(passwordEncoder.encode(request.password()));

        User savedUser = userRepository.save(user);
        return mapToResponse(savedUser);
    }

    @Transactional
    public UserResponse updateCurrentUser(UpdateUserRequest request) {
        User user = currentUserService.getCurrentUser();

        if (!user.getEmail().equalsIgnoreCase(request.email())) {
            userRepository.findUserByEmail(request.email()).ifPresent(existing -> {
                throw new ResourceConflictException("This email address is already registered to another user.");
            });
            user.setEmail(request.email());
        }

        user.setFirstName(request.firstName());
        user.setLastName(request.lastName());

        User updatedUser = userRepository.save(user);
        return mapToResponse(updatedUser);
    }

    @Transactional
    public void deleteCurrentUser() {
        User user = currentUserService.getCurrentUser();
        userRepository.delete(user);
    }

    private UserResponse mapToResponse(User user) {
        return new UserResponse(
                user.getId(),
                user.getFirstName(),
                user.getLastName(),
                user.getFullName(),
                user.getEmail(),
                user.getUsername(),
                user.getCreatedAt()
        );
    }
}
