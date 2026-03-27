(define compose (lambdas (f g) (lambda (x) (f (g x)))))

(define flip (lambda (f) (lambdas (x y) (@ f y x))))

(define const_fn (lambda (x) (lambda (_) x)))

(define twice (lambda (f) (lambda (x) (f (f x)))))

(define apply_n (lambdas (f n x)
  (match n
    ((O) x)
    ((S n~) (@ apply_n f n~ (f x))))))
