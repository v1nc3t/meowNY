package com.meowny.server.service;

import com.meowny.commons.entity.User;
import com.meowny.server.dto.user.CreateUserRequest;
import com.meowny.server.dto.user.UpdateUserRequest;
import com.meowny.server.dto.user.UserResponse;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.UserRepository;
import org.springframework.security.crypto.bcrypt.BCryptPasswordEncoder;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
public class UserService {

    private final UserRepository userRepository;
    private final BCryptPasswordEncoder passwordEncoder;

    public UserService(UserRepository userRepository, BCryptPasswordEncoder passwordEncoder) {
        this.userRepository = userRepository;
        this.passwordEncoder = passwordEncoder;
    }

    @Transactional(readOnly = true)
    public UserResponse getUserById(Long id) {
        User user = userRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("User not fouund with ID:" + id));
        return mapToResponse(user);
    }

    @Transactional
    public UserResponse createUser(CreateUserRequest request) {
        userRepository.findUserByEmail(request.email()).ifPresent(u -> {
            throw new ResourceConflictException("An account with this email address already exists.");
        });

        userRepository.findUserByUsername(request.username()).ifPresent(u -> {
            throw new ResourceConflictException("This username is already taken.");
        });

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
    public UserResponse updateUser(Long id, UpdateUserRequest request) {
        User user = userRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("User not found with ID: " + id));

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
    public void deleteUser(Long id) {
        if (!userRepository.existsById(id)) {
            throw new IllegalArgumentException("User not found with ID: " + id);
        }
        userRepository.deleteById(id);
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
