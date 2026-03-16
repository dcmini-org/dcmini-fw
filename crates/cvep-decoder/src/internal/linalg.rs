use libm::{atan2f, cosf, sinf, sqrtf};

pub(crate) fn cholesky_lower<const N: usize>(
    matrix: &[[f32; N]; N],
) -> Option<[[f32; N]; N]> {
    let mut out = [[0.0; N]; N];
    let mut row = 0;
    while row < N {
        let mut col = 0;
        while col <= row {
            let mut sum = matrix[row][col];
            let mut k = 0;
            while k < col {
                sum -= out[row][k] * out[col][k];
                k += 1;
            }

            if row == col {
                if sum <= 1.0e-12 {
                    return None;
                }
                out[row][col] = sqrtf(sum);
            } else {
                out[row][col] = sum / out[col][col];
            }
            col += 1;
        }
        row += 1;
    }
    Some(out)
}

pub(crate) fn solve_lower_left<const N: usize, const M: usize>(
    lower: &[[f32; N]; N],
    rhs: &[[f32; M]; N],
) -> [[f32; M]; N] {
    let mut out = [[0.0; M]; N];
    let mut col = 0;
    while col < M {
        let mut row = 0;
        while row < N {
            let mut sum = rhs[row][col];
            let mut k = 0;
            while k < row {
                sum -= lower[row][k] * out[k][col];
                k += 1;
            }
            out[row][col] = sum / lower[row][row];
            row += 1;
        }
        col += 1;
    }
    out
}

pub(crate) fn solve_lower_right_transpose<const R: usize, const N: usize>(
    lhs: &[[f32; N]; R],
    lower: &[[f32; N]; N],
) -> [[f32; N]; R] {
    let mut out = [[0.0; N]; R];
    let mut row = 0;
    while row < R {
        let mut col = 0;
        while col < N {
            let mut sum = lhs[row][col];
            let mut k = 0;
            while k < col {
                sum -= out[row][k] * lower[col][k];
                k += 1;
            }
            out[row][col] = sum / lower[col][col];
            col += 1;
        }
        row += 1;
    }
    out
}

pub(crate) fn solve_lower_transpose_vec<const N: usize>(
    lower: &[[f32; N]; N],
    rhs: &[f32; N],
) -> [f32; N] {
    let mut out = [0.0; N];
    let mut idx = N;
    while idx > 0 {
        let row = idx - 1;
        let mut sum = rhs[row];
        let mut col = row + 1;
        while col < N {
            sum -= lower[col][row] * out[col];
            col += 1;
        }
        out[row] = sum / lower[row][row];
        idx -= 1;
    }
    out
}

pub(crate) fn jacobi_eigen<const N: usize>(
    matrix: &[[f32; N]; N],
) -> ([f32; N], [[f32; N]; N]) {
    let mut a = *matrix;
    let mut v = [[0.0; N]; N];
    let mut i = 0;
    while i < N {
        v[i][i] = 1.0;
        i += 1;
    }

    let mut iter = 0;
    while iter < 50 * N * N {
        let mut p = 0usize;
        let mut q = 1usize.min(N.saturating_sub(1));
        let mut max = 0.0f32;
        let mut i = 0;
        while i < N {
            let mut j = i + 1;
            while j < N {
                let value = a[i][j].abs();
                if value > max {
                    max = value;
                    p = i;
                    q = j;
                }
                j += 1;
            }
            i += 1;
        }

        if max <= 1.0e-8 {
            break;
        }

        let app = a[p][p];
        let aqq = a[q][q];
        let apq = a[p][q];
        let phi = 0.5 * atan2f(2.0 * apq, aqq - app);
        let c = cosf(phi);
        let s = sinf(phi);

        let mut k = 0;
        while k < N {
            if k != p && k != q {
                let aik = a[p][k];
                let akq = a[q][k];
                a[p][k] = c * aik - s * akq;
                a[k][p] = a[p][k];
                a[q][k] = s * aik + c * akq;
                a[k][q] = a[q][k];
            }
            k += 1;
        }

        a[p][p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
        a[q][q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
        a[p][q] = 0.0;
        a[q][p] = 0.0;

        let mut k = 0;
        while k < N {
            let vip = v[k][p];
            let viq = v[k][q];
            v[k][p] = c * vip - s * viq;
            v[k][q] = s * vip + c * viq;
            k += 1;
        }

        iter += 1;
    }

    let mut values = [0.0; N];
    let mut i = 0;
    while i < N {
        values[i] = a[i][i];
        i += 1;
    }
    (values, v)
}

pub(crate) fn gram_matrix<const R: usize, const C: usize>(
    matrix: &[[f32; C]; R],
) -> [[f32; R]; R] {
    let mut out = [[0.0; R]; R];
    let mut i = 0;
    while i < R {
        let mut j = 0;
        while j < R {
            let mut sum = 0.0f32;
            let mut k = 0;
            while k < C {
                sum += matrix[i][k] * matrix[j][k];
                k += 1;
            }
            out[i][j] = sum;
            j += 1;
        }
        i += 1;
    }
    out
}

pub(crate) fn dominant_singular_vectors<const R: usize, const C: usize>(
    matrix: &[[f32; C]; R],
) -> ([f32; R], [f32; C], f32) {
    let gram = gram_matrix(matrix);
    let (values, vectors) = jacobi_eigen(&gram);

    let mut best_idx = 0usize;
    let mut best_value = values[0];
    let mut idx = 1;
    while idx < R {
        if values[idx] > best_value {
            best_value = values[idx];
            best_idx = idx;
        }
        idx += 1;
    }

    let sigma = sqrtf(best_value.max(0.0));
    if sigma <= 1.0e-12 {
        return ([0.0; R], [0.0; C], 0.0);
    }

    let mut u = [0.0; R];
    let mut row = 0;
    while row < R {
        u[row] = vectors[row][best_idx];
        row += 1;
    }

    let mut v = mat_t_vec_mul_rect(matrix, &u);
    let mut col = 0;
    while col < C {
        v[col] /= sigma;
        col += 1;
    }

    (u, v, sigma)
}

fn mat_t_vec_mul_rect<const R: usize, const C: usize>(
    matrix: &[[f32; C]; R],
    vector: &[f32; R],
) -> [f32; C] {
    let mut out = [0.0; C];
    let mut col = 0;
    while col < C {
        let mut sum = 0.0f32;
        let mut row = 0;
        while row < R {
            sum += matrix[row][col] * vector[row];
            row += 1;
        }
        out[col] = sum;
        col += 1;
    }
    out
}
