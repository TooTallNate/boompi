// CSS
import styles from '@styles/connect-bluetooth.module.css';

interface ConnectBluetoothProps {
	name: string;
}

export default function ConnectBluetooth({ name }: ConnectBluetoothProps) {
	return (
		<div className={styles.bt}>
			<h3 name="instructions">
				To play music, connect your bluetooth device to:
			</h3>
			<h1 name="name">{name}</h1>
		</div>
	);
}
